use anyhow::Result;
use seatshell_config::load_config;
use slint::{ModelRc, VecModel};
use std::process::Stdio;
use tracing_subscriber::EnvFilter;

mod apps;

slint::include_modules!();

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = load_config()?;
    let apps = apps::discover_apps();
    tracing::info!(count = apps.len(), "discovered launcher apps");

    let launcher_apps: Vec<LauncherApp> = apps
        .iter()
        .map(|app| LauncherApp {
            id: app.id.clone().into(),
            name: app.name.clone().into(),
            exec: app.exec.clone().into(),
        })
        .collect();

    let ui = AppWindow::new()?;
    ui.set_launcher_apps(ModelRc::new(VecModel::from(launcher_apps)));
    ui.set_user_sessions(ModelRc::new(VecModel::from(current_sessions())));

    ui.set_panel_height(config.panel.height as i32);
    ui.set_accent(config.desktop.accent.into());
    ui.set_overview_enabled(config.overview.enabled);
    ui.set_show_launcher(std::env::args().any(|arg| arg == "--launcher"));
    ui.set_show_overview(std::env::args().any(|arg| arg == "--overview"));

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

    let launch_apps = apps.clone();
    ui.on_launch_app(move |app_id| {
        tracing::info!(app_id = %app_id, "launcher app requested");

        let Some(app) = launch_apps.iter().find(|app| app_id == app.id) else {
            tracing::warn!(app_id = %app_id, "launcher app id was not found");
            return;
        };

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
    });

    ui.run()?;
    Ok(())
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
