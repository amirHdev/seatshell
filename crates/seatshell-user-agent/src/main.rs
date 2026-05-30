use anyhow::{Context, Result};
use seatshell_protocol::{USER_AGENT_BUS_NAME_PREFIX, USER_AGENT_OBJECT_PATH, user_agent};
use std::process::Stdio;
use tokio::process::Command;
use tracing_subscriber::EnvFilter;
use zbus::{Connection, connection::Builder, fdo::DBusProxy, interface, message::Header};

struct UserAgent {
    username: String,
    uid: u32,
}

impl UserAgent {
    fn new() -> Self {
        let username = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
        let uid = current_uid();

        Self { username, uid }
    }
}

#[interface(name = "org.seatshell.UserAgent")]
impl UserAgent {
    async fn launch_command(
        &self,
        command: Vec<String>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.authorize_caller(connection, &header).await?;

        let Some((program, args)) = command.split_first() else {
            return Err(zbus::fdo::Error::InvalidArgs(
                "command cannot be empty".into(),
            ));
        };

        tracing::info!(program, args = ?args, "launching command");

        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| zbus::fdo::Error::Failed(format!("failed to launch command: {err}")))?;

        Ok(())
    }

    async fn launch_desktop_file(
        &self,
        desktop_file_id: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.authorize_caller(connection, &header).await?;

        if desktop_file_id.trim().is_empty() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "desktop file id cannot be empty".into(),
            ));
        }

        tracing::info!(desktop_file_id, "launching desktop file via gtk-launch");

        Command::new("gtk-launch")
            .arg(&desktop_file_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| {
                zbus::fdo::Error::Failed(format!(
                    "failed to launch desktop file {desktop_file_id}: {err}"
                ))
            })?;

        Ok(())
    }

    async fn get_session_info(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<(String, u32, String, String, String, bool)> {
        self.authorize_caller(connection, &header).await?;

        let session_id = std::env::var("XDG_SESSION_ID").unwrap_or_else(|_| "unknown".into());
        let seat = std::env::var("XDG_SEAT").unwrap_or_else(|_| "seat0".into());

        Ok((
            session_id,
            self.uid,
            self.username.clone(),
            seat,
            "active".into(),
            false,
        ))
    }

    async fn list_running_apps(&self) -> zbus::fdo::Result<Vec<String>> {
        let uid = self.uid.to_string();
        let output = Command::new("ps")
            .args(["-u", uid.as_str(), "-o", "comm="])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|err| zbus::fdo::Error::Failed(format!("failed to list processes: {err}")))?;

        if !output.status.success() {
            return Err(zbus::fdo::Error::Failed(format!(
                "ps exited with {}",
                output.status
            )));
        }

        let names = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter(|line| *line != "ps")
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        Ok(names)
    }
}

impl UserAgent {
    async fn authorize_caller(
        &self,
        connection: &Connection,
        header: &Header<'_>,
    ) -> zbus::fdo::Result<()> {
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

        if caller_uid == self.uid {
            Ok(())
        } else {
            tracing::warn!(
                caller_uid,
                service_uid = self.uid,
                "rejected user-agent D-Bus caller"
            );
            Err(zbus::fdo::Error::AccessDenied(format!(
                "caller uid {caller_uid} is not allowed to access user-agent uid {}",
                self.uid
            )))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let uid = current_uid();
    let bus_name = format!("{USER_AGENT_BUS_NAME_PREFIX}.u{uid}");

    if std::env::args().any(|arg| arg == "--dry-run") {
        println!("SeatShell user agent dry run");
        println!("bus name: {bus_name}");
        println!("object path: {USER_AGENT_OBJECT_PATH}");
        println!("interface: {}", user_agent::INTERFACE);
        println!("methods:");
        println!("  - LaunchCommand");
        println!("  - LaunchDesktopFile");
        println!("  - ListRunningApps");
        println!("  - GetSessionInfo");
        return Ok(());
    }

    let _connection = Builder::session()?
        .name(bus_name.as_str())?
        .serve_at(USER_AGENT_OBJECT_PATH, UserAgent::new())?
        .build()
        .await
        .context("failed to register SeatShell user-agent D-Bus service")?;

    tracing::info!(
        bus_name,
        object_path = USER_AGENT_OBJECT_PATH,
        interface = user_agent::INTERFACE,
        "SeatShell user agent registered"
    );

    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}
