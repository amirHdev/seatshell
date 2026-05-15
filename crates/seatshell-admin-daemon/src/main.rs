use anyhow::Result;
use seatshell_common::{SessionInfo, SessionState, UserInfo};
use seatshell_config::load_config;
use seatshell_protocol::{ADMIN_BUS_NAME, ADMIN_OBJECT_PATH, admin};
use std::fs;
use tracing_subscriber::EnvFilter;
use zbus::{connection::Builder, interface};

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
    async fn list_users(&self) -> zbus::fdo::Result<Vec<(u32, String, String, bool)>> {
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
    ) -> zbus::fdo::Result<Vec<(String, u32, String, String, String, bool)>> {
        Ok(list_sessions()
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
        println!("detected sessions: {}", list_sessions().len());
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
    let uid = unsafe { libc::getuid() };
    let username = std::env::var("USER").unwrap_or_else(|_| "unknown".into());

    UserInfo {
        uid,
        is_admin: uid == 0 || user_in_group(&username, admin_group),
        username,
        display_name: None,
    }
}

fn list_sessions() -> Vec<SessionInfo> {
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

fn session_state_name(state: &SessionState) -> &'static str {
    match state {
        SessionState::Active => "active",
        SessionState::Online => "online",
        SessionState::Closing => "closing",
        SessionState::Inactive => "inactive",
        SessionState::Unknown => "unknown",
    }
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
        let sessions = list_sessions();

        assert_eq!(sessions.len(), 1);
        assert!(!sessions[0].username.is_empty());
        assert!(!sessions[0].id.is_empty());
        assert_eq!(sessions[0].state, SessionState::Active);
    }
}
