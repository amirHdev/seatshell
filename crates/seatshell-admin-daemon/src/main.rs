use anyhow::{Context, Result};
use seatshell_common::{SessionInfo, SessionState, UserInfo};
use seatshell_config::load_config;
use seatshell_protocol::{ADMIN_BUS_NAME, ADMIN_OBJECT_PATH, admin};
use std::fs;
use tracing_subscriber::EnvFilter;
use zbus::{
    Connection, Proxy, connection::Builder, fdo::DBusProxy, interface, message::Header,
    zvariant::OwnedObjectPath,
};

struct AdminService {
    allowed_group: String,
}

impl AdminService {
    fn new(allowed_group: String) -> Self {
        Self { allowed_group }
    }
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
        println!("planned privileged methods:");
        println!("  - {}", admin::OPEN_APP_AS_USER);
        println!("  - {}", admin::LOCK_SESSION);
        println!("  - {}", admin::LOGOUT_SESSION);
        println!("  - {}", admin::SEND_MESSAGE);
        println!("  - {}", admin::GET_SESSION_STATE);
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
            AdminService::new(config.admin.allowed_group.clone()),
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
    let service_uid = current_uid();

    if caller_uid == service_uid {
        Ok(())
    } else {
        tracing::warn!(caller_uid, service_uid, "rejected D-Bus caller");
        Err(zbus::fdo::Error::AccessDenied(format!(
            "caller uid {caller_uid} is not allowed to access SeatShell admin uid {service_uid}"
        )))
    }
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
