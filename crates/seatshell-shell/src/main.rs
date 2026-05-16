use anyhow::Result;
use chrono::Local;
use seatshell_config::load_config;
use seatshell_protocol::{ADMIN_BUS_NAME, ADMIN_OBJECT_PATH, admin};
use slint::{ModelRc, Timer, TimerMode, VecModel};
use std::{cell::RefCell, process::Stdio, rc::Rc, time::Duration};
use tracing_subscriber::EnvFilter;
use zbus::Proxy;

mod apps;

slint::include_modules!();

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = std::env::args().collect::<Vec<_>>();
    let windowed = args.iter().any(|arg| arg == "--windowed");

    let config = load_config()?;
    let apps = apps::discover_apps();
    tracing::info!(count = apps.len(), "discovered launcher apps");

    let ui = AppWindow::new()?;
    ui.set_launcher_apps(launcher_model(&apps));
    ui.set_launcher_count(apps.len() as i32);
    ui.set_user_sessions(ModelRc::new(VecModel::from(user_sessions())));

    ui.set_panel_height(config.panel.height as i32);
    ui.set_accent(config.desktop.accent.into());
    ui.set_clock_text(clock_text().into());
    ui.set_status_text(format!("{} applications available", apps.len()).into());
    ui.set_overview_enabled(config.overview.enabled);
    ui.set_show_launcher(args.iter().any(|arg| arg == "--launcher"));
    ui.set_show_overview(args.iter().any(|arg| arg == "--overview"));
    ui.set_windowed(windowed);

    let weak = ui.as_weak();
    let clock_timer = Timer::default();
    clock_timer.start(TimerMode::Repeated, Duration::from_secs(30), move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_clock_text(clock_text().into());
        }
    });

    let weak = ui.as_weak();
    ui.on_show_desktop(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_show_launcher(false);
            ui.set_show_overview(false);
        }
    });

    let weak = ui.as_weak();
    ui.on_toggle_launcher(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_show_launcher(!ui.get_show_launcher());
            ui.set_show_overview(false);
        }
    });

    let weak = ui.as_weak();
    ui.on_toggle_overview(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_show_overview(!ui.get_show_overview());
            ui.set_show_launcher(false);
        }
    });

    let all_apps = Rc::new(apps);
    let filtered_apps = Rc::new(RefCell::new(all_apps.as_ref().clone()));

    let weak = ui.as_weak();
    {
        let all_apps = Rc::clone(&all_apps);
        let filtered_apps = Rc::clone(&filtered_apps);
        ui.on_launcher_search_changed(move |query| {
            let matches = filter_apps(&all_apps, query.as_str());
            *filtered_apps.borrow_mut() = matches.clone();

            if let Some(ui) = weak.upgrade() {
                ui.set_launcher_query(query);
                ui.set_launcher_count(matches.len() as i32);
                ui.set_launcher_apps(launcher_model(&matches));
            }
        });
    }

    {
        let filtered_apps = Rc::clone(&filtered_apps);
        ui.on_launcher_accept_search(move || {
            if let Some(app) = filtered_apps.borrow().first() {
                launch_app(app);
            }
        });
    }

    let launch_apps = Rc::clone(&all_apps);
    ui.on_launch_app(move |app_id| {
        tracing::info!(app_id = %app_id, "launcher app requested");

        let Some(app) = launch_apps.iter().find(|app| app_id == app.id) else {
            tracing::warn!(app_id = %app_id, "launcher app id was not found");
            return;
        };

        launch_app(app);
    });

    ui.run()?;
    Ok(())
}

fn launcher_model(apps: &[apps::AppEntry]) -> ModelRc<LauncherApp> {
    ModelRc::new(VecModel::from(
        apps.iter()
            .map(|app| LauncherApp {
                id: app.id.clone().into(),
                name: app.name.clone().into(),
                exec: app.exec.clone().into(),
            })
            .collect::<Vec<_>>(),
    ))
}

fn filter_apps(apps: &[apps::AppEntry], query: &str) -> Vec<apps::AppEntry> {
    let query = query.trim().to_lowercase();

    if query.is_empty() {
        return apps.to_vec();
    }

    apps.iter()
        .filter(|app| {
            app.name.to_lowercase().contains(&query)
                || app.exec.to_lowercase().contains(&query)
                || app.id.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

fn launch_app(app: &apps::AppEntry) {
    let parts = if app.argv.is_empty() {
        apps::split_command(&app.exec).unwrap_or_default()
    } else {
        app.argv.clone()
    };

    let Some((program, args)) = parts.split_first() else {
        tracing::warn!("launcher command was empty");
        return;
    };

    if let Err(err) = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        tracing::warn!(program, error = %err, "failed to launch app");
    }
}

fn clock_text() -> String {
    Local::now().format("%a %H:%M").to_string()
}

fn user_sessions() -> Vec<UserSession> {
    match admin_sessions() {
        Ok(sessions) if !sessions.is_empty() => sessions,
        Ok(_) => {
            tracing::warn!("admin daemon returned no sessions, using local session fallback");
            current_sessions()
        }
        Err(err) => {
            tracing::debug!(error = %err, "could not load sessions from admin daemon");
            current_sessions()
        }
    }
}

fn admin_sessions() -> Result<Vec<UserSession>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;

    runtime.block_on(async {
        let connection = zbus::Connection::session().await?;
        let proxy = Proxy::new(
            &connection,
            ADMIN_BUS_NAME,
            ADMIN_OBJECT_PATH,
            admin::INTERFACE,
        )
        .await?;

        let sessions = tokio::time::timeout(
            Duration::from_millis(750),
            proxy.call::<_, _, Vec<(String, u32, String, String, String, bool)>>(
                admin::LIST_SESSIONS,
                &(),
            ),
        )
        .await??;

        Ok(sessions.into_iter().map(session_row_to_ui).collect())
    })
}

fn session_row_to_ui(
    (id, _uid, username, seat, state, locked): (String, u32, String, String, String, bool),
) -> UserSession {
    let state_text = if locked {
        format!("locked on {seat}")
    } else {
        format!("{state} on {seat}")
    };

    UserSession {
        username: username.into(),
        state: state_text.into(),
        action: format!("session {id}").into(),
    }
}

fn current_sessions() -> Vec<UserSession> {
    let username = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
    let session_id = std::env::var("XDG_SESSION_ID")
        .or_else(|_| std::env::var("TERM_SESSION_ID"))
        .unwrap_or_else(|_| "local".into());
    let seat = std::env::var("XDG_SEAT").unwrap_or_else(|_| "seat0".into());

    vec![UserSession {
        username: username.into(),
        state: format!("active on {seat}").into(),
        action: format!("session {session_id}").into(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_row_maps_locked_state_for_overview() {
        let session = session_row_to_ui((
            "session-1".into(),
            1000,
            "seat".into(),
            "seat0".into(),
            "active".into(),
            true,
        ));

        assert_eq!(session.username.as_str(), "seat");
        assert_eq!(session.state.as_str(), "locked on seat0");
        assert_eq!(session.action.as_str(), "session session-1");
    }

    #[test]
    fn session_row_maps_unlocked_state_for_overview() {
        let session = session_row_to_ui((
            "session-2".into(),
            1000,
            "shell".into(),
            "seat1".into(),
            "online".into(),
            false,
        ));

        assert_eq!(session.username.as_str(), "shell");
        assert_eq!(session.state.as_str(), "online on seat1");
        assert_eq!(session.action.as_str(), "session session-2");
    }

    #[test]
    fn launcher_filter_matches_name_exec_and_id() {
        let apps = vec![
            apps::AppEntry {
                id: "org.example.Terminal.desktop".into(),
                name: "Terminal".into(),
                exec: "konsole".into(),
                argv: vec!["konsole".into()],
            },
            apps::AppEntry {
                id: "org.example.Files.desktop".into(),
                name: "Files".into(),
                exec: "dolphin".into(),
                argv: vec!["dolphin".into()],
            },
        ];

        assert_eq!(filter_apps(&apps, "").len(), 2);
        assert_eq!(filter_apps(&apps, "term")[0].name, "Terminal");
        assert_eq!(filter_apps(&apps, "dolphin")[0].name, "Files");
        assert!(filter_apps(&apps, "missing").is_empty());
    }
}
