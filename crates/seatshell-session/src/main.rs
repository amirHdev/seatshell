use anyhow::{Context, Result, anyhow, bail};
use seatshell_protocol::{ADMIN_BUS_NAME, SHELL_BUS_NAME, USER_AGENT_BUS_NAME_PREFIX};
use std::{
    collections::VecDeque,
    env,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::{Duration, Instant},
};
use tokio::process::{Child, Command};
use tracing_subscriber::{EnvFilter, fmt::writer::MakeWriterExt};
use zbus::{Connection, fdo::DBusProxy, names::BusName};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(12);
const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(250);
const LABWC_GRACE_PERIOD: Duration = Duration::from_millis(1500);
const RESTART_BACKOFF: Duration = Duration::from_secs(2);
const RESTART_WINDOW: Duration = Duration::from_secs(30);
const MAX_RESTARTS_PER_WINDOW: usize = 3;

#[derive(Clone, Debug)]
struct RuntimePaths {
    bin_dir: PathBuf,
    share_dir: PathBuf,
    state_dir: PathBuf,
    log_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChildRole {
    Labwc,
    AdminDaemon,
    UserAgent,
    Shell,
}

impl ChildRole {
    fn label(self) -> &'static str {
        match self {
            Self::Labwc => "labwc",
            Self::AdminDaemon => "admin-daemon",
            Self::UserAgent => "user-agent",
            Self::Shell => "shell",
        }
    }

    fn binary(self) -> &'static str {
        match self {
            Self::Labwc => "labwc",
            Self::AdminDaemon => "seatshell-admin-daemon",
            Self::UserAgent => "seatshell-user-agent",
            Self::Shell => "seatshell-shell",
        }
    }

    fn log_name(self) -> &'static str {
        match self {
            Self::Labwc => "labwc",
            Self::AdminDaemon => "seatshell-admin-daemon",
            Self::UserAgent => "seatshell-user-agent",
            Self::Shell => "seatshell-shell",
        }
    }

    fn args(self, windowed: bool) -> Vec<&'static str> {
        match self {
            Self::Shell if windowed => vec!["--windowed"],
            _ => Vec::new(),
        }
    }

    fn should_spawn(self, windowed: bool) -> bool {
        !matches!(self, Self::Labwc) || !windowed
    }

    fn startup_required(self) -> bool {
        true
    }

    fn restartable(self) -> bool {
        !matches!(self, Self::Labwc)
    }

    fn readiness_name(self, uid: u32) -> Option<String> {
        match self {
            Self::Labwc => None,
            Self::AdminDaemon => Some(ADMIN_BUS_NAME.into()),
            Self::UserAgent => Some(format!("{USER_AGENT_BUS_NAME_PREFIX}.u{uid}")),
            Self::Shell => Some(SHELL_BUS_NAME.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExitPolicy {
    FailSession,
    Restart,
}

#[derive(Debug)]
struct ManagedChild {
    role: ChildRole,
    child: Child,
    healthy: bool,
    started_at: Instant,
    restart_attempts: VecDeque<Instant>,
    restart_count: usize,
}

impl ManagedChild {
    fn new(role: ChildRole, child: Child) -> Self {
        Self {
            role,
            child,
            healthy: false,
            started_at: Instant::now(),
            restart_attempts: VecDeque::new(),
            restart_count: 0,
        }
    }

    fn mark_restarted(&mut self, child: Child) {
        self.child = child;
        self.healthy = false;
        self.started_at = Instant::now();
        self.restart_count += 1;
        self.restart_attempts.push_back(Instant::now());
    }

    fn can_restart_now(&mut self) -> bool {
        restart_budget_available(&mut self.restart_attempts, Instant::now())
    }
}

struct SessionSupervisor {
    runtime_paths: RuntimePaths,
    connection: Connection,
    children: Vec<ManagedChild>,
    windowed: bool,
    uid: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let runtime_paths = runtime_paths()?;
    let _log_guard = init_logging(&runtime_paths.log_dir)?;
    tracing::info!(
        state_dir = %runtime_paths.state_dir.display(),
        log_dir = %runtime_paths.log_dir.display(),
        "initialized SeatShell runtime paths"
    );

    let args = env::args().collect::<Vec<_>>();
    let dry_run = args.iter().any(|arg| arg == "--dry-run");
    let dev_dry_run = args.iter().any(|arg| arg == "--dev-dry-run");
    let windowed = args.iter().any(|arg| arg == "--windowed");

    if dry_run {
        print_runtime_plan(&runtime_paths, windowed);
        return Ok(());
    }

    if dev_dry_run {
        print_dev_plan(&runtime_paths);
        return Ok(());
    }

    ensure_workspace_binaries(
        &runtime_paths,
        &[
            "seatshell-admin-daemon",
            "seatshell-user-agent",
            "seatshell-shell",
        ],
    )?;

    let connection = Connection::session()
        .await
        .context("failed to connect to the session D-Bus")?;

    let uid = unsafe { libc::getuid() };
    let mut supervisor = SessionSupervisor::new(runtime_paths, connection, windowed, uid).await?;
    let result = supervisor.run().await;
    supervisor.shutdown_all().await;
    result
}

impl SessionSupervisor {
    async fn new(
        runtime_paths: RuntimePaths,
        connection: Connection,
        windowed: bool,
        uid: u32,
    ) -> Result<Self> {
        let mut children = Vec::new();
        let launch_order = [
            ChildRole::Labwc,
            ChildRole::AdminDaemon,
            ChildRole::UserAgent,
            ChildRole::Shell,
        ];

        tracing::info!(?launch_order, "starting SeatShell session children");

        for role in launch_order {
            if !role.should_spawn(windowed) {
                continue;
            }

            let child = spawn_role(role, &runtime_paths, windowed)
                .await
                .with_context(|| format!("failed to start {}", role.label()))?;
            children.push(ManagedChild::new(role, child));
        }

        Ok(Self {
            runtime_paths,
            connection,
            children,
            windowed,
            uid,
        })
    }

    async fn run(&mut self) -> Result<()> {
        self.wait_for_startup().await?;
        tracing::info!("SeatShell session startup completed successfully");
        self.supervise().await
    }

    async fn wait_for_startup(&mut self) -> Result<()> {
        let deadline = Instant::now() + STARTUP_TIMEOUT;

        loop {
            self.refresh_readiness().await?;
            if self.startup_complete() {
                self.log_startup_summary("ready");
                return Ok(());
            }

            if let Some(reason) = self.check_for_exits(true).await? {
                self.log_startup_summary(&reason);
                return Err(anyhow!(reason));
            }

            if Instant::now() >= deadline {
                let reason = self.startup_timeout_reason();
                self.log_startup_summary(&reason);
                return Err(anyhow!(reason));
            }

            tokio::time::sleep(STARTUP_POLL_INTERVAL).await;
        }
    }

    async fn supervise(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("received shutdown signal");
                    return Ok(());
                }
                _ = tokio::time::sleep(STARTUP_POLL_INTERVAL) => {}
            }

            self.refresh_readiness().await?;
            if let Some(reason) = self.check_for_exits(false).await? {
                return Err(anyhow!(reason));
            }
        }
    }

    async fn refresh_readiness(&mut self) -> Result<()> {
        for managed in &mut self.children {
            if managed.healthy {
                continue;
            }

            let became_ready = match managed.role.readiness_name(self.uid) {
                Some(name) => bus_name_owned(&self.connection, &name)
                    .await
                    .with_context(|| {
                        format!("failed readiness check for {}", managed.role.label())
                    })?,
                None => managed.started_at.elapsed() >= LABWC_GRACE_PERIOD,
            };

            if became_ready {
                managed.healthy = true;
                tracing::info!(role = managed.role.label(), "service became healthy");
            }
        }

        Ok(())
    }

    fn startup_complete(&self) -> bool {
        self.children
            .iter()
            .filter(|managed| managed.role.startup_required())
            .all(|managed| managed.healthy)
    }

    fn startup_timeout_reason(&self) -> String {
        let pending = pending_startup_roles(
            &self
                .children
                .iter()
                .map(|managed| (managed.role, managed.healthy))
                .collect::<Vec<_>>(),
        );

        format!("startup timed out waiting for: {}", pending.join(", "))
    }

    async fn check_for_exits(&mut self, startup_phase: bool) -> Result<Option<String>> {
        for index in 0..self.children.len() {
            let status = {
                let managed = &mut self.children[index];
                managed.child.try_wait().with_context(|| {
                    format!("failed to query child status for {}", managed.role.label())
                })?
            };

            let Some(status) = status else {
                continue;
            };

            let role = self.children[index].role;
            tracing::warn!(
                role = role.label(),
                ?status,
                startup_phase,
                "session child exited"
            );

            match exit_policy(role, startup_phase, self.children[index].healthy) {
                ExitPolicy::FailSession => {
                    return Ok(Some(failure_message(role, startup_phase, status)));
                }
                ExitPolicy::Restart => {
                    if !self.children[index].can_restart_now() {
                        let reason = format!(
                            "{} crash-looped; restart limit reached within {:?}",
                            role.label(),
                            RESTART_WINDOW
                        );
                        tracing::error!(role = role.label(), "{reason}");
                        return Ok(Some(reason));
                    }

                    tracing::info!(
                        role = role.label(),
                        backoff_ms = RESTART_BACKOFF.as_millis(),
                        "restarting session child"
                    );
                    tokio::time::sleep(RESTART_BACKOFF).await;
                    let child = spawn_role(role, &self.runtime_paths, self.windowed)
                        .await
                        .with_context(|| format!("failed to restart {}", role.label()))?;
                    self.children[index].mark_restarted(child);
                    tracing::info!(
                        role = role.label(),
                        restart_count = self.children[index].restart_count,
                        "session child restarted"
                    );
                }
            }
        }

        Ok(None)
    }

    fn log_startup_summary(&self, outcome: &str) {
        tracing::info!(outcome, "startup summary");
        for managed in &self.children {
            tracing::info!(
                role = managed.role.label(),
                healthy = managed.healthy,
                restart_count = managed.restart_count,
                started_ms_ago = managed.started_at.elapsed().as_millis() as u64,
                "startup child state"
            );
        }
    }

    async fn shutdown_all(&mut self) {
        for managed in self.children.iter_mut().rev() {
            let Some(pid) = managed.child.id() else {
                continue;
            };

            tracing::info!(role = managed.role.label(), pid, "stopping session child");
            if managed.child.start_kill().is_err() {
                continue;
            }
            let _ = managed.child.wait().await;
        }
    }
}

fn runtime_paths() -> Result<RuntimePaths> {
    let exe_dir = env::current_exe()?
        .parent()
        .context("session binary has no parent directory")?
        .to_path_buf();

    let bin_dir = env::var_os("SEATSHELL_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or(exe_dir);

    let share_dir = env::var_os("SEATSHELL_SHARE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            bin_dir
                .parent()
                .unwrap_or(&bin_dir)
                .join("share")
                .join("seatshell")
        });

    let state_dir = env::var_os("SEATSHELL_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_dir);
    let log_dir = env::var_os("SEATSHELL_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| state_dir.join("logs"));
    fs::create_dir_all(&log_dir)
        .with_context(|| format!("failed to create log directory {}", log_dir.display()))?;

    Ok(RuntimePaths {
        bin_dir,
        share_dir,
        state_dir,
        log_dir,
    })
}

fn print_runtime_plan(paths: &RuntimePaths, windowed: bool) {
    if windowed {
        println!(
            "{}",
            binary_path(paths, ChildRole::AdminDaemon.binary()).display()
        );
        println!(
            "{}",
            binary_path(paths, ChildRole::UserAgent.binary()).display()
        );
        println!(
            "{} --windowed",
            binary_path(paths, ChildRole::Shell.binary()).display()
        );
    } else {
        println!("labwc");
        println!(
            "{}",
            binary_path(paths, ChildRole::AdminDaemon.binary()).display()
        );
        println!(
            "{}",
            binary_path(paths, ChildRole::UserAgent.binary()).display()
        );
        println!(
            "{}",
            binary_path(paths, ChildRole::Shell.binary()).display()
        );
    }
}

fn print_dev_plan(paths: &RuntimePaths) {
    println!("labwc");
    println!(
        "{} --dry-run",
        binary_path(paths, ChildRole::AdminDaemon.binary()).display()
    );
    println!(
        "{} --dry-run",
        binary_path(paths, ChildRole::UserAgent.binary()).display()
    );
    println!(
        "{}",
        binary_path(paths, ChildRole::Shell.binary()).display()
    );
}

async fn spawn_role(role: ChildRole, paths: &RuntimePaths, windowed: bool) -> Result<Child> {
    match role {
        ChildRole::Labwc => spawn_labwc(paths).await,
        _ => spawn_binary(paths, role.binary(), &role.args(windowed), role.log_name()).await,
    }
}

async fn spawn_labwc(paths: &RuntimePaths) -> Result<Child> {
    let mut command = Command::new("labwc");
    configure_command(&mut command, paths);
    configure_process_logs(&mut command, paths, ChildRole::Labwc.log_name())?;

    let labwc_dir = paths.share_dir.join("labwc");
    if labwc_dir.is_dir() {
        command.env("LABWC_CONFIG_DIR", labwc_dir);
    }

    tracing::info!("spawning labwc");
    Ok(command.stdin(Stdio::null()).spawn()?)
}

async fn spawn_binary(
    paths: &RuntimePaths,
    binary: &str,
    args: &[&str],
    log_name: &str,
) -> Result<Child> {
    let path = binary_path(paths, binary);
    let mut command = if path.exists() {
        let mut command = Command::new(&path);
        command.args(args);
        command
    } else if let Some(workspace_root) = find_workspace_root() {
        let mut command = Command::new("cargo");
        command
            .arg("run")
            .arg("-p")
            .arg(binary)
            .arg("--")
            .args(args)
            .current_dir(workspace_root);
        command
    } else {
        let mut command = Command::new(binary);
        command.args(args);
        command
    };

    configure_command(&mut command, paths);
    configure_process_logs(&mut command, paths, log_name)?;
    tracing::info!(binary, args = ?args, "spawning process");
    Ok(command.stdin(Stdio::null()).spawn()?)
}

fn configure_command(command: &mut Command, paths: &RuntimePaths) {
    command.env("SEATSHELL_BIN_DIR", &paths.bin_dir);
    command.env("SEATSHELL_SHARE_DIR", &paths.share_dir);
    command.env("SEATSHELL_SESSION_MANAGED", "1");
    command.env("SEATSHELL_STATE_DIR", &paths.state_dir);
    command.env("SEATSHELL_LOG_DIR", &paths.log_dir);
    command.env("XDG_CURRENT_DESKTOP", "SeatShell");
    command.env("XDG_SESSION_DESKTOP", "SeatShell");
    command.env("PATH", prepend_path(&paths.bin_dir));
}

fn prepend_path(bin_dir: &Path) -> String {
    let current = env::var("PATH").unwrap_or_default();
    if current.is_empty() {
        return bin_dir.display().to_string();
    }

    format!("{}:{current}", bin_dir.display())
}

fn binary_path(paths: &RuntimePaths, binary: &str) -> PathBuf {
    paths.bin_dir.join(binary)
}

fn find_workspace_root() -> Option<PathBuf> {
    let current = env::current_dir().ok()?;

    current
        .ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file() && dir.join("crates").is_dir())
        .map(Path::to_path_buf)
}

fn ensure_workspace_binaries(paths: &RuntimePaths, binaries: &[&str]) -> Result<()> {
    let missing = binaries
        .iter()
        .copied()
        .filter(|binary| !binary_path(paths, binary).exists())
        .collect::<Vec<_>>();

    if missing.is_empty() {
        return Ok(());
    }

    let Some(workspace_root) = find_workspace_root() else {
        return Ok(());
    };

    tracing::info!(?missing, "building missing SeatShell workspace binaries");
    let status = StdCommand::new("cargo")
        .arg("build")
        .args(missing.iter().flat_map(|binary| ["-p", *binary]))
        .current_dir(workspace_root)
        .status()
        .context("failed to run cargo build for missing binaries")?;

    if !status.success() {
        bail!("cargo build for SeatShell binaries exited with {status}");
    }

    Ok(())
}

fn init_logging(log_dir: &Path) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let file_appender = tracing_appender::rolling::never(log_dir, "session.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(false)
        .with_writer(io::stderr.and(non_blocking))
        .init();

    Ok(guard)
}

fn configure_process_logs(command: &mut Command, paths: &RuntimePaths, name: &str) -> Result<()> {
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.log_dir.join(format!("{name}.log")))
        .with_context(|| format!("failed to open log file for {name}"))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("failed to clone log file handle for {name}"))?;

    command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    Ok(())
}

fn default_state_dir() -> PathBuf {
    if let Some(state_home) = env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state_home).join("seatshell");
    }

    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local")
        .join("state")
        .join("seatshell")
}

async fn bus_name_owned(connection: &Connection, name: &str) -> Result<bool> {
    let bus_name = BusName::try_from(name).context("invalid D-Bus bus name for readiness check")?;
    let result = tokio::time::timeout(Duration::from_millis(1200), async {
        let proxy = DBusProxy::new(connection).await?;
        proxy.name_has_owner(bus_name).await
    })
    .await;

    match result {
        Ok(Ok(owned)) => Ok(owned),
        Ok(Err(error)) => Err(error).context("failed D-Bus ownership query"),
        Err(_) => Ok(false),
    }
}

fn restart_budget_available(restart_attempts: &mut VecDeque<Instant>, now: Instant) -> bool {
    while restart_attempts
        .front()
        .is_some_and(|attempt| now.duration_since(*attempt) > RESTART_WINDOW)
    {
        restart_attempts.pop_front();
    }

    restart_attempts.len() < MAX_RESTARTS_PER_WINDOW
}

fn exit_policy(role: ChildRole, startup_phase: bool, healthy: bool) -> ExitPolicy {
    if role == ChildRole::Labwc {
        return ExitPolicy::FailSession;
    }

    if startup_phase && role == ChildRole::Shell && !healthy {
        return ExitPolicy::FailSession;
    }

    if role.restartable() {
        ExitPolicy::Restart
    } else {
        ExitPolicy::FailSession
    }
}

fn failure_message(
    role: ChildRole,
    startup_phase: bool,
    status: std::process::ExitStatus,
) -> String {
    if startup_phase {
        format!(
            "{} exited before SeatShell startup completed with status {status}",
            role.label()
        )
    } else {
        format!(
            "{} exited and cannot be recovered (status {status})",
            role.label()
        )
    }
}

fn pending_startup_roles(children: &[(ChildRole, bool)]) -> Vec<&'static str> {
    children
        .iter()
        .filter(|(role, healthy)| role.startup_required() && !healthy)
        .map(|(role, _)| role.label())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labwc_exit_always_fails_session() {
        assert_eq!(
            exit_policy(ChildRole::Labwc, true, false),
            ExitPolicy::FailSession
        );
        assert_eq!(
            exit_policy(ChildRole::Labwc, false, true),
            ExitPolicy::FailSession
        );
    }

    #[test]
    fn shell_exit_during_startup_fails_session() {
        assert_eq!(
            exit_policy(ChildRole::Shell, true, false),
            ExitPolicy::FailSession
        );
    }

    #[test]
    fn user_services_restart_in_startup_and_steady_state() {
        assert_eq!(
            exit_policy(ChildRole::AdminDaemon, true, false),
            ExitPolicy::Restart
        );
        assert_eq!(
            exit_policy(ChildRole::UserAgent, false, true),
            ExitPolicy::Restart
        );
        assert_eq!(
            exit_policy(ChildRole::Shell, false, true),
            ExitPolicy::Restart
        );
    }

    #[test]
    fn restart_window_is_bounded() {
        let mut restart_attempts = VecDeque::new();

        assert!(restart_budget_available(
            &mut restart_attempts,
            Instant::now()
        ));
        restart_attempts.push_back(Instant::now());
        restart_attempts.push_back(Instant::now());
        restart_attempts.push_back(Instant::now());
        assert!(!restart_budget_available(
            &mut restart_attempts,
            Instant::now()
        ));
    }

    #[test]
    fn startup_timeout_lists_unhealthy_roles() {
        let pending = pending_startup_roles(&[
            (ChildRole::Shell, false),
            (ChildRole::UserAgent, true),
            (ChildRole::AdminDaemon, false),
        ]);

        assert_eq!(pending, vec!["shell", "admin-daemon"]);
    }

    #[test]
    fn restart_window_recovers_after_old_attempts_expire() {
        let mut restart_attempts = VecDeque::from([
            Instant::now() - RESTART_WINDOW - Duration::from_secs(1),
            Instant::now() - RESTART_WINDOW - Duration::from_millis(10),
            Instant::now() - RESTART_WINDOW - Duration::from_millis(1),
        ]);

        assert!(restart_budget_available(
            &mut restart_attempts,
            Instant::now()
        ));
        assert!(restart_attempts.is_empty());
    }
}
