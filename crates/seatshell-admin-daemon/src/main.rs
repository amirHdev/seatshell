use anyhow::{Context, Result};
use seatshell_common::{SessionInfo, SessionState, UserInfo};
use seatshell_config::load_config;
use seatshell_protocol::{
    ADMIN_BUS_NAME, ADMIN_OBJECT_PATH, DESKTOP_NOTIFICATIONS_BUS_NAME,
    DESKTOP_NOTIFICATIONS_OBJECT_PATH, admin, desktop_notifications,
};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
use tracing_subscriber::EnvFilter;
use zbus::{
    Connection, Proxy,
    connection::Builder,
    fdo::DBusProxy,
    interface,
    message::Header,
    zvariant::{OwnedObjectPath, OwnedValue},
};

struct AdminService {
    allowed_group: String,
    require_reauth: bool,
    log_actions: bool,
    allow_logout_user: bool,
    allow_lock_user: bool,
    allow_restart_system: bool,
    allow_power_off_system: bool,
    notify_user_on_admin_action: bool,
}

impl AdminService {
    fn new(
        allowed_group: String,
        require_reauth: bool,
        log_actions: bool,
        allow_logout_user: bool,
        allow_lock_user: bool,
        allow_restart_system: bool,
        allow_power_off_system: bool,
        notify_user_on_admin_action: bool,
    ) -> Self {
        Self {
            allowed_group,
            require_reauth,
            log_actions,
            allow_logout_user,
            allow_lock_user,
            allow_restart_system,
            allow_power_off_system,
            notify_user_on_admin_action,
        }
    }

    fn runtime_policy(&self) -> RuntimePolicy {
        RuntimePolicy {
            require_reauth: self.require_reauth,
            allow_logout_user: self.allow_logout_user,
            allow_lock_user: self.allow_lock_user,
            allow_restart_system: self.allow_restart_system,
            allow_power_off_system: self.allow_power_off_system,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdminAction {
    LockSession,
    LogoutSession,
    RestartSystem,
    PowerOffSystem,
    SendMessage,
}

impl AdminAction {
    fn action_id(self) -> &'static str {
        match self {
            Self::LockSession => "org.seatshell.admin.lock-session",
            Self::LogoutSession => "org.seatshell.admin.logout-session",
            Self::RestartSystem => "org.seatshell.admin.restart-system",
            Self::PowerOffSystem => "org.seatshell.admin.power-off-system",
            Self::SendMessage => "org.seatshell.admin.send-message",
        }
    }

    fn method_name(self) -> &'static str {
        match self {
            Self::LockSession => admin::LOCK_SESSION,
            Self::LogoutSession => admin::LOGOUT_SESSION,
            Self::RestartSystem => admin::RESTART_SYSTEM,
            Self::PowerOffSystem => admin::POWER_OFF_SYSTEM,
            Self::SendMessage => admin::SEND_MESSAGE,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CallerIdentity {
    uid: u32,
    pid: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AuthorizedAction {
    caller: CallerIdentity,
    target: SessionInfo,
    reauthenticated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RuntimePolicy {
    require_reauth: bool,
    allow_logout_user: bool,
    allow_lock_user: bool,
    allow_restart_system: bool,
    allow_power_off_system: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AuthorizationPlan {
    reauthenticate: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthorizationFailure {
    Disabled,
    AdminRequired,
}

#[interface(name = "org.seatshell.Admin")]
impl AdminService {
    async fn list_users(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<Vec<(u32, String, String, bool)>> {
        authorize_same_uid(connection, &header).await?;

        Ok(list_users(&self.allowed_group)
            .into_iter()
            .map(|user| {
                (
                    user.uid,
                    user.username,
                    user.display_name.unwrap_or_default(),
                    user.is_admin,
                )
            })
            .collect())
    }

    async fn list_sessions(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<Vec<(String, u32, String, String, String, bool)>> {
        authorize_same_uid(connection, &header).await?;

        Ok(discover_sessions()
            .await
            .into_iter()
            .map(|session| {
                (
                    session.id,
                    session.uid,
                    session.username,
                    session.seat,
                    session_state_name(&session.state).to_string(),
                    session.locked,
                )
            })
            .collect())
    }

    async fn get_policy_group(&self) -> String {
        self.allowed_group.clone()
    }

    async fn lock_session(
        &self,
        session_id: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let authorized = self
            .authorize_action(connection, &header, AdminAction::LockSession, session_id)
            .await?;
        lock_session_with_logind(session_id)
            .await
            .map_err(to_fdo_failed("failed to lock session"))?;
        self.audit_action(
            AdminAction::LockSession,
            &authorized,
            "allowed",
            session_id,
            None,
        );
        if self.notify_user_on_admin_action {
            let _ = post_desktop_notification(
                "SeatShell",
                &format!("Locked session {}", authorized.target.username),
                &format!("Session {} was locked.", authorized.target.id),
            )
            .await;
        }
        Ok(())
    }

    async fn logout_session(
        &self,
        session_id: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let authorized = self
            .authorize_action(connection, &header, AdminAction::LogoutSession, session_id)
            .await?;
        logout_session_with_logind(session_id)
            .await
            .map_err(to_fdo_failed("failed to log out session"))?;
        self.audit_action(
            AdminAction::LogoutSession,
            &authorized,
            "allowed",
            session_id,
            None,
        );
        Ok(())
    }

    async fn restart_system(
        &self,
        session_id: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let authorized = self
            .authorize_action(connection, &header, AdminAction::RestartSystem, session_id)
            .await?;
        restart_system_with_logind()
            .await
            .map_err(to_fdo_failed("failed to restart system"))?;
        self.audit_action(
            AdminAction::RestartSystem,
            &authorized,
            "allowed",
            session_id,
            None,
        );
        Ok(())
    }

    async fn power_off_system(
        &self,
        session_id: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let authorized = self
            .authorize_action(connection, &header, AdminAction::PowerOffSystem, session_id)
            .await?;
        power_off_system_with_logind()
            .await
            .map_err(to_fdo_failed("failed to power off system"))?;
        self.audit_action(
            AdminAction::PowerOffSystem,
            &authorized,
            "allowed",
            session_id,
            None,
        );
        Ok(())
    }

    async fn send_message(
        &self,
        session_id: &str,
        message: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let message = message.trim();
        if message.is_empty() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "message cannot be empty".into(),
            ));
        }

        let authorized = self
            .authorize_action(connection, &header, AdminAction::SendMessage, session_id)
            .await?;
        send_message_to_session(&authorized.target, message)
            .await
            .map_err(to_fdo_failed("failed to deliver session message"))?;
        self.audit_action(
            AdminAction::SendMessage,
            &authorized,
            "allowed",
            session_id,
            Some(message),
        );
        Ok(())
    }

    async fn get_session_state(
        &self,
        session_id: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<(String, bool)> {
        authorize_same_uid(connection, &header).await?;
        session_state_lookup(discover_sessions().await.as_slice(), session_id)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = load_config()?;

    if std::env::args().any(|arg| arg == "--dry-run") {
        println!("SeatShell admin daemon dry run");
        println!("bus name: {ADMIN_BUS_NAME}");
        println!("object path: {ADMIN_OBJECT_PATH}");
        println!("interface: {}", admin::INTERFACE);
        println!("allowed group: {}", config.admin.allowed_group);
        println!("registered methods:");
        println!("  - {}", admin::LIST_USERS);
        println!("  - {}", admin::LIST_SESSIONS);
        println!("  - {}", admin::GET_POLICY_GROUP);
        println!("  - {}", admin::LOCK_SESSION);
        println!("  - {}", admin::LOGOUT_SESSION);
        println!("  - {}", admin::RESTART_SYSTEM);
        println!("  - {}", admin::POWER_OFF_SYSTEM);
        println!("  - {}", admin::SEND_MESSAGE);
        println!("  - {}", admin::GET_SESSION_STATE);
        println!("planned privileged methods:");
        println!("  - {}", admin::OPEN_APP_AS_USER);
        println!("  - {}", admin::REQUEST_PREVIEW);
        println!(
            "detected users: {}",
            list_users(&config.admin.allowed_group).len()
        );
        println!("detected sessions: {}", discover_sessions().await.len());
        return Ok(());
    }

    let _connection = Builder::session()?
        .name(ADMIN_BUS_NAME)?
        .serve_at(
            ADMIN_OBJECT_PATH,
            AdminService::new(
                config.admin.allowed_group.clone(),
                config.admin.require_reauth,
                config.admin.log_actions,
                config.control.allow_logout_user,
                config.control.allow_lock_user,
                config.control.allow_restart_system,
                config.control.allow_power_off_system,
                config.privacy.notify_user_on_admin_action,
            ),
        )?
        .build()
        .await?;

    tracing::info!(
        bus_name = ADMIN_BUS_NAME,
        object_path = ADMIN_OBJECT_PATH,
        allowed_group = %config.admin.allowed_group,
        "SeatShell admin daemon registered"
    );

    tokio::signal::ctrl_c().await?;
    Ok(())
}

impl AdminService {
    async fn authorize_action(
        &self,
        connection: &Connection,
        header: &Header<'_>,
        action: AdminAction,
        session_id: &str,
    ) -> zbus::fdo::Result<AuthorizedAction> {
        self.ensure_action_enabled(action)?;
        let caller = caller_identity(connection, header).await?;
        let target = find_session_by_id(discover_sessions().await.as_slice(), session_id)?.clone();

        let current = current_user(&self.allowed_group);
        let plan = authorization_plan(
            self.runtime_policy(),
            action,
            caller.uid,
            target.uid,
            current.is_admin,
        )
        .map_err(|failure| match failure {
            AuthorizationFailure::Disabled => zbus::fdo::Error::AccessDenied(format!(
                "{} is disabled by SeatShell policy",
                action.method_name()
            )),
            AuthorizationFailure::AdminRequired => zbus::fdo::Error::AccessDenied(format!(
                "user {} is not allowed to control session {}",
                current.username, target.id
            )),
        })?;

        if plan.reauthenticate {
            require_polkit_reauth(&caller, action)
                .await
                .map_err(to_fdo_denied("caller re-authentication failed"))?;
        }

        Ok(AuthorizedAction {
            caller,
            target,
            reauthenticated: plan.reauthenticate,
        })
    }

    fn ensure_action_enabled(&self, action: AdminAction) -> zbus::fdo::Result<()> {
        if action_enabled(self.runtime_policy(), action) {
            Ok(())
        } else {
            Err(zbus::fdo::Error::AccessDenied(format!(
                "{} is disabled by SeatShell policy",
                action.method_name()
            )))
        }
    }

    fn audit_action(
        &self,
        action: AdminAction,
        authorized: &AuthorizedAction,
        outcome: &str,
        session_id: &str,
        message: Option<&str>,
    ) {
        if !self.log_actions {
            return;
        }

        if let Err(error) = append_audit_entry(
            action,
            authorized,
            outcome,
            session_id,
            message.unwrap_or_default(),
        ) {
            tracing::warn!(%error, "failed to write SeatShell admin audit entry");
        }
    }
}

fn list_users(admin_group: &str) -> Vec<UserInfo> {
    let mut users = parse_passwd_users(admin_group);

    if users.is_empty() {
        users.push(current_user(admin_group));
    }

    users.sort_by(|left, right| {
        left.uid
            .cmp(&right.uid)
            .then(left.username.cmp(&right.username))
    });
    users.dedup_by_key(|user| user.uid);
    users
}

fn parse_passwd_users(admin_group: &str) -> Vec<UserInfo> {
    let Ok(content) = fs::read_to_string("/etc/passwd") else {
        return Vec::new();
    };

    content
        .lines()
        .filter_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            if fields.len() < 7 {
                return None;
            }

            let uid = fields[2].parse::<u32>().ok()?;
            if uid < 500 && fields[0] != "root" {
                return None;
            }

            Some(UserInfo {
                uid,
                username: fields[0].to_string(),
                display_name: display_name(fields[4]),
                is_admin: fields[0] == "root" || user_in_group(fields[0], admin_group),
            })
        })
        .collect()
}

fn display_name(gecos: &str) -> Option<String> {
    let name = gecos.split(',').next().unwrap_or_default().trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn user_in_group(username: &str, group: &str) -> bool {
    let Ok(content) = fs::read_to_string("/etc/group") else {
        return false;
    };

    content.lines().any(|line| {
        let fields = line.split(':').collect::<Vec<_>>();
        fields.first() == Some(&group)
            && fields
                .get(3)
                .is_some_and(|members| members.split(',').any(|member| member == username))
    })
}

fn current_user(admin_group: &str) -> UserInfo {
    let uid = current_uid();
    let username = std::env::var("USER").unwrap_or_else(|_| "unknown".into());

    UserInfo {
        uid,
        is_admin: uid == 0 || user_in_group(&username, admin_group),
        username,
        display_name: None,
    }
}

async fn discover_sessions() -> Vec<SessionInfo> {
    match logind_sessions().await {
        Ok(sessions) if !sessions.is_empty() => sessions,
        Ok(_) => {
            tracing::warn!("logind returned no sessions, using local session fallback");
            local_sessions()
        }
        Err(err) => {
            tracing::debug!(error = %err, "could not load sessions from logind");
            local_sessions()
        }
    }
}

async fn logind_sessions() -> Result<Vec<SessionInfo>> {
    let connection = Connection::system()
        .await
        .context("failed to connect to the system bus")?;
    let manager = Proxy::new(
        &connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    .context("failed to create logind manager proxy")?;

    let sessions = manager
        .call::<_, _, Vec<(String, u32, String, String, OwnedObjectPath)>>("ListSessions", &())
        .await
        .context("failed to list logind sessions")?;

    let mut discovered = Vec::with_capacity(sessions.len());
    for row in sessions {
        match logind_session_info(&connection, row).await {
            Ok(session) => discovered.push(session),
            Err(err) => tracing::warn!(error = %err, "could not load logind session details"),
        }
    }

    Ok(discovered)
}

async fn logind_session_info(
    connection: &Connection,
    (id, uid, username, seat, path): (String, u32, String, String, OwnedObjectPath),
) -> Result<SessionInfo> {
    let session = Proxy::new(
        connection,
        "org.freedesktop.login1",
        path,
        "org.freedesktop.login1.Session",
    )
    .await
    .context("failed to create logind session proxy")?;

    let state = session
        .get_property::<String>("State")
        .await
        .map(|state| session_state_from_logind(&state))
        .unwrap_or(SessionState::Unknown);
    let locked = session
        .get_property::<bool>("LockedHint")
        .await
        .unwrap_or(false);

    Ok(SessionInfo {
        id,
        uid,
        username,
        seat,
        state,
        locked,
    })
}

fn local_sessions() -> Vec<SessionInfo> {
    let user = current_user("wheel");
    let session_id = std::env::var("XDG_SESSION_ID")
        .or_else(|_| std::env::var("TERM_SESSION_ID"))
        .unwrap_or_else(|_| format!("local-{}", user.uid));
    let seat = std::env::var("XDG_SEAT").unwrap_or_else(|_| "seat0".into());

    vec![SessionInfo {
        id: session_id,
        uid: user.uid,
        username: user.username,
        seat,
        state: SessionState::Active,
        locked: false,
    }]
}

fn find_session_by_id<'a>(
    sessions: &'a [SessionInfo],
    session_id: &str,
) -> zbus::fdo::Result<&'a SessionInfo> {
    sessions
        .iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| zbus::fdo::Error::Failed(format!("session {session_id} was not found")))
}

fn session_state_lookup(
    sessions: &[SessionInfo],
    session_id: &str,
) -> zbus::fdo::Result<(String, bool)> {
    let session = find_session_by_id(sessions, session_id)?;
    Ok((session_state_name(&session.state).into(), session.locked))
}

fn session_state_from_logind(state: &str) -> SessionState {
    match state {
        "active" => SessionState::Active,
        "online" => SessionState::Online,
        "closing" => SessionState::Closing,
        "inactive" => SessionState::Inactive,
        _ => SessionState::Unknown,
    }
}

fn session_state_name(state: &SessionState) -> &'static str {
    match state {
        SessionState::Active => "active",
        SessionState::Online => "online",
        SessionState::Closing => "closing",
        SessionState::Inactive => "inactive",
        SessionState::Unknown => "unknown",
    }
}

async fn authorize_same_uid(connection: &Connection, header: &Header<'_>) -> zbus::fdo::Result<()> {
    caller_identity(connection, header).await.map(|_| ())
}

async fn caller_identity(
    connection: &Connection,
    header: &Header<'_>,
) -> zbus::fdo::Result<CallerIdentity> {
    let sender = header
        .sender()
        .ok_or_else(|| zbus::fdo::Error::AccessDenied("missing D-Bus sender".into()))?;
    let proxy = DBusProxy::new(connection).await.map_err(|err| {
        zbus::fdo::Error::Failed(format!("failed to create D-Bus daemon proxy: {err}"))
    })?;
    let caller_uid = proxy
        .get_connection_unix_user(sender.clone().into())
        .await
        .map_err(|err| {
            zbus::fdo::Error::AccessDenied(format!("could not verify caller identity: {err}"))
        })?;
    let caller_pid = proxy
        .get_connection_unix_process_id(sender.to_owned().into())
        .await
        .map_err(|err| {
            zbus::fdo::Error::AccessDenied(format!("could not verify caller process: {err}"))
        })?;
    let service_uid = current_uid();

    if caller_uid == service_uid {
        Ok(CallerIdentity {
            uid: caller_uid,
            pid: caller_pid,
        })
    } else {
        tracing::warn!(caller_uid, caller_pid, service_uid, "rejected D-Bus caller");
        Err(zbus::fdo::Error::AccessDenied(format!(
            "caller uid {caller_uid} is not allowed to access SeatShell admin uid {service_uid}"
        )))
    }
}

async fn require_polkit_reauth(caller: &CallerIdentity, action: AdminAction) -> Result<()> {
    let status = Command::new("pkcheck")
        .args([
            "--action-id",
            action.action_id(),
            "--process",
            &caller.pid.to_string(),
            "--allow-user-interaction",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run pkcheck")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("pkcheck exited with {status}");
    }
}

async fn lock_session_with_logind(session_id: &str) -> Result<()> {
    let status = Command::new("loginctl")
        .args(["lock-session", session_id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run loginctl lock-session")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("loginctl lock-session exited with {status}");
    }
}

async fn logout_session_with_logind(session_id: &str) -> Result<()> {
    let status = Command::new("loginctl")
        .args(["terminate-session", session_id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run loginctl terminate-session")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("loginctl terminate-session exited with {status}");
    }
}

async fn restart_system_with_logind() -> Result<()> {
    run_system_power_command([["loginctl", "reboot"], ["systemctl", "reboot"]]).await
}

async fn power_off_system_with_logind() -> Result<()> {
    run_system_power_command([["loginctl", "poweroff"], ["systemctl", "poweroff"]]).await
}

async fn run_system_power_command<const N: usize>(commands: [[&str; 2]; N]) -> Result<()> {
    let mut errors = Vec::new();

    for [program, arg] in commands {
        match Command::new(program)
            .arg(arg)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => errors.push(format!("{program} {arg} exited with {status}")),
            Err(error) => errors.push(format!("failed to run {program} {arg}: {error}")),
        }
    }

    anyhow::bail!("{}", errors.join("; "))
}

async fn send_message_to_session(target: &SessionInfo, message: &str) -> Result<()> {
    if !message_delivery_allowed(target.uid, current_uid(), &target.id, &current_session_id()) {
        anyhow::bail!("message delivery is currently limited to the current user's active session");
    }

    post_desktop_notification("SeatShell", "SeatShell admin message", message).await
}

async fn post_desktop_notification(app_name: &str, summary: &str, body: &str) -> Result<()> {
    let connection = Connection::session()
        .await
        .context("failed to connect to the session bus for notifications")?;
    let proxy = Proxy::new(
        &connection,
        DESKTOP_NOTIFICATIONS_BUS_NAME,
        DESKTOP_NOTIFICATIONS_OBJECT_PATH,
        desktop_notifications::INTERFACE,
    )
    .await
    .context("failed to create desktop notification proxy")?;

    proxy
        .call_method(
            desktop_notifications::NOTIFY,
            &(
                app_name,
                0_u32,
                "",
                summary,
                body,
                Vec::<String>::new(),
                HashMap::<String, OwnedValue>::new(),
                10_000_i32,
            ),
        )
        .await
        .context("desktop notification notify call failed")?;

    Ok(())
}

fn current_session_id() -> String {
    std::env::var("XDG_SESSION_ID")
        .or_else(|_| std::env::var("TERM_SESSION_ID"))
        .unwrap_or_else(|_| format!("local-{}", current_uid()))
}

fn append_audit_entry(
    action: AdminAction,
    authorized: &AuthorizedAction,
    outcome: &str,
    session_id: &str,
    message: &str,
) -> Result<()> {
    let path = admin_audit_log_path();
    append_audit_entry_to_path(&path, action, authorized, outcome, session_id, message)
}

fn append_audit_entry_to_path(
    path: &Path,
    action: AdminAction,
    authorized: &AuthorizedAction,
    outcome: &str,
    session_id: &str,
    message: &str,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create admin audit dir {}", parent.display()))?;
    }

    let line = audit_log_line(action, authorized, outcome, session_id, message);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open admin audit log {}", path.display()))?;
    writeln!(file, "{line}")
        .with_context(|| format!("failed to write admin audit log {}", path.display()))?;
    Ok(())
}

fn audit_log_line(
    action: AdminAction,
    authorized: &AuthorizedAction,
    outcome: &str,
    session_id: &str,
    message: &str,
) -> String {
    let timestamp = audit_timestamp();
    format!(
        "ts={timestamp} action={} outcome={outcome} caller_uid={} caller_pid={} target_uid={} target_session={} reauth={} message={}",
        action.method_name(),
        authorized.caller.uid,
        authorized.caller.pid,
        authorized.target.uid,
        session_id,
        authorized.reauthenticated,
        sanitize_audit_field(message),
    )
}

fn admin_audit_log_path() -> PathBuf {
    if let Some(log_dir) = std::env::var_os("SEATSHELL_LOG_DIR") {
        return PathBuf::from(log_dir).join("admin-actions.log");
    }

    if let Some(state_dir) = std::env::var_os("SEATSHELL_STATE_DIR") {
        return PathBuf::from(state_dir)
            .join("logs")
            .join("admin-actions.log");
    }

    default_state_dir().join("logs").join("admin-actions.log")
}

fn default_state_dir() -> PathBuf {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state_home).join("seatshell");
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local")
        .join("state")
        .join("seatshell")
}

fn audit_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn sanitize_audit_field(value: &str) -> String {
    value.replace('\n', "\\n").replace('\r', "")
}

fn action_enabled(policy: RuntimePolicy, action: AdminAction) -> bool {
    match action {
        AdminAction::LockSession => policy.allow_lock_user,
        AdminAction::LogoutSession => policy.allow_logout_user,
        AdminAction::RestartSystem => policy.allow_restart_system,
        AdminAction::PowerOffSystem => policy.allow_power_off_system,
        AdminAction::SendMessage => true,
    }
}

fn authorization_plan(
    policy: RuntimePolicy,
    action: AdminAction,
    caller_uid: u32,
    target_uid: u32,
    current_user_is_admin: bool,
) -> std::result::Result<AuthorizationPlan, AuthorizationFailure> {
    if !action_enabled(policy, action) {
        return Err(AuthorizationFailure::Disabled);
    }

    if target_uid == caller_uid {
        return Ok(AuthorizationPlan {
            reauthenticate: false,
        });
    }

    if !current_user_is_admin {
        return Err(AuthorizationFailure::AdminRequired);
    }

    Ok(AuthorizationPlan {
        reauthenticate: policy.require_reauth,
    })
}

fn message_delivery_allowed(
    target_uid: u32,
    current_uid: u32,
    target_session_id: &str,
    current_session_id: &str,
) -> bool {
    target_uid == current_uid && target_session_id == current_session_id
}

fn to_fdo_failed(
    prefix: &'static str,
) -> impl Fn(anyhow::Error) -> zbus::fdo::Error + Copy + 'static {
    move |error| zbus::fdo::Error::Failed(format!("{prefix}: {error}"))
}

fn to_fdo_denied(
    prefix: &'static str,
) -> impl Fn(anyhow::Error) -> zbus::fdo::Error + Copy + 'static {
    move |error| zbus::fdo::Error::AccessDenied(format!("{prefix}: {error}"))
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_policy() -> RuntimePolicy {
        RuntimePolicy {
            require_reauth: true,
            allow_logout_user: true,
            allow_lock_user: true,
            allow_restart_system: true,
            allow_power_off_system: true,
        }
    }

    fn sample_authorized_action() -> AuthorizedAction {
        AuthorizedAction {
            caller: CallerIdentity {
                uid: 1000,
                pid: 4242,
            },
            target: SessionInfo {
                id: "seat-1".into(),
                uid: 1001,
                username: "alice".into(),
                seat: "seat0".into(),
                state: SessionState::Active,
                locked: false,
            },
            reauthenticated: true,
        }
    }

    fn sample_sessions() -> Vec<SessionInfo> {
        vec![
            SessionInfo {
                id: "seat-1".into(),
                uid: 1000,
                username: "alice".into(),
                seat: "seat0".into(),
                state: SessionState::Active,
                locked: false,
            },
            SessionInfo {
                id: "seat-2".into(),
                uid: 1001,
                username: "bob".into(),
                seat: "seat1".into(),
                state: SessionState::Unknown,
                locked: true,
            },
        ]
    }

    #[test]
    fn display_name_uses_first_gecos_field() {
        assert_eq!(
            display_name("Seat Shell,Room 1,555-0100"),
            Some("Seat Shell".into())
        );
        assert_eq!(display_name(""), None);
    }

    #[test]
    fn session_state_names_match_protocol_strings() {
        assert_eq!(session_state_name(&SessionState::Active), "active");
        assert_eq!(session_state_name(&SessionState::Online), "online");
        assert_eq!(session_state_name(&SessionState::Closing), "closing");
        assert_eq!(session_state_name(&SessionState::Inactive), "inactive");
        assert_eq!(session_state_name(&SessionState::Unknown), "unknown");
    }

    #[test]
    fn list_sessions_has_current_user_session() {
        let sessions = local_sessions();

        assert_eq!(sessions.len(), 1);
        assert!(!sessions[0].username.is_empty());
        assert!(!sessions[0].id.is_empty());
        assert_eq!(sessions[0].state, SessionState::Active);
    }

    #[test]
    fn maps_logind_session_states() {
        assert_eq!(session_state_from_logind("active"), SessionState::Active);
        assert_eq!(session_state_from_logind("online"), SessionState::Online);
        assert_eq!(session_state_from_logind("closing"), SessionState::Closing);
        assert_eq!(
            session_state_from_logind("inactive"),
            SessionState::Inactive
        );
        assert_eq!(session_state_from_logind("weird"), SessionState::Unknown);
    }

    #[test]
    fn admin_action_method_names_match_protocol_strings() {
        assert_eq!(AdminAction::LockSession.method_name(), admin::LOCK_SESSION);
        assert_eq!(
            AdminAction::LogoutSession.method_name(),
            admin::LOGOUT_SESSION
        );
        assert_eq!(
            AdminAction::RestartSystem.method_name(),
            admin::RESTART_SYSTEM
        );
        assert_eq!(
            AdminAction::PowerOffSystem.method_name(),
            admin::POWER_OFF_SYSTEM
        );
        assert_eq!(AdminAction::SendMessage.method_name(), admin::SEND_MESSAGE);
    }

    #[test]
    fn sanitize_audit_field_flattens_newlines() {
        assert_eq!(
            sanitize_audit_field("hello\nworld\r\nagain"),
            "hello\\nworld\\nagain"
        );
    }

    #[test]
    fn authorization_plan_allows_same_user_without_reauth() {
        let plan = authorization_plan(sample_policy(), AdminAction::LockSession, 1000, 1000, false)
            .expect("same-user action should be allowed");

        assert!(!plan.reauthenticate);
    }

    #[test]
    fn authorization_plan_requires_admin_for_cross_user_action() {
        let failure =
            authorization_plan(sample_policy(), AdminAction::LockSession, 1000, 1001, false)
                .expect_err("cross-user non-admin action should be denied");

        assert_eq!(failure, AuthorizationFailure::AdminRequired);
    }

    #[test]
    fn authorization_plan_requires_reauth_for_cross_user_admin_action() {
        let plan = authorization_plan(
            sample_policy(),
            AdminAction::LogoutSession,
            1000,
            1001,
            true,
        )
        .expect("cross-user admin action should be allowed");

        assert!(plan.reauthenticate);
    }

    #[test]
    fn authorization_plan_honors_disabled_policy() {
        let failure = authorization_plan(
            RuntimePolicy {
                require_reauth: true,
                allow_logout_user: false,
                allow_lock_user: true,
                allow_restart_system: true,
                allow_power_off_system: true,
            },
            AdminAction::LogoutSession,
            1000,
            1000,
            true,
        )
        .expect_err("disabled action should be denied");

        assert_eq!(failure, AuthorizationFailure::Disabled);
    }

    #[test]
    fn authorization_plan_honors_disabled_power_policy() {
        let failure = authorization_plan(
            RuntimePolicy {
                require_reauth: true,
                allow_logout_user: true,
                allow_lock_user: true,
                allow_restart_system: false,
                allow_power_off_system: true,
            },
            AdminAction::RestartSystem,
            1000,
            1000,
            true,
        )
        .expect_err("disabled restart action should be denied");

        assert_eq!(failure, AuthorizationFailure::Disabled);
    }

    #[test]
    fn message_delivery_allowed_only_for_current_matching_session() {
        assert!(message_delivery_allowed(1000, 1000, "seat-1", "seat-1"));
        assert!(!message_delivery_allowed(1001, 1000, "seat-1", "seat-1"));
        assert!(!message_delivery_allowed(1000, 1000, "seat-2", "seat-1"));
    }

    #[test]
    fn session_state_lookup_returns_state_and_locked_flag() {
        let state = session_state_lookup(&sample_sessions(), "seat-2").expect("lookup should work");
        assert_eq!(state, ("unknown".into(), true));
    }

    #[test]
    fn session_state_lookup_rejects_missing_session() {
        let error = session_state_lookup(&sample_sessions(), "missing")
            .expect_err("missing session should fail");
        assert!(error.to_string().contains("session missing was not found"));
    }

    #[test]
    fn find_session_by_id_returns_matching_session() {
        let sessions = sample_sessions();
        let session = find_session_by_id(&sessions, "seat-1").expect("find session");
        assert_eq!(session.username, "alice");
    }

    #[test]
    fn append_audit_entry_to_path_writes_sanitized_line() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("seatshell-audit-{unique}.log"));
        let authorized = sample_authorized_action();

        append_audit_entry_to_path(
            &path,
            AdminAction::SendMessage,
            &authorized,
            "allowed",
            "seat-1",
            "hello\nworld",
        )
        .expect("audit entry should be written");

        let content = fs::read_to_string(&path).expect("read audit file");
        assert!(content.contains(&format!(
            "action={}",
            AdminAction::SendMessage.method_name()
        )));
        assert!(content.contains("caller_uid=1000"));
        assert!(content.contains("target_uid=1001"));
        assert!(content.contains("message=hello\\nworld"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn fdo_failed_wrapper_preserves_prefix_and_error_text() {
        let error = to_fdo_failed("lock failed")(anyhow::anyhow!("boom"));
        assert!(error.to_string().contains("lock failed: boom"));
    }

    #[test]
    fn fdo_denied_wrapper_preserves_prefix_and_error_text() {
        let error = to_fdo_denied("reauth failed")(anyhow::anyhow!("denied"));
        assert!(error.to_string().contains("reauth failed: denied"));
    }
}
