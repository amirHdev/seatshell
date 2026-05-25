use anyhow::{Context, Result, bail};
use chrono::Local;
use seatshell_common::config::PanelPosition;
use seatshell_config::load_config;
use seatshell_notifications::{Notification, NotificationUrgency};
use seatshell_protocol::{
    ADMIN_BUS_NAME, ADMIN_OBJECT_PATH, SHELL_BUS_NAME, SHELL_OBJECT_PATH, admin, shell,
};
use slint::{Image, ModelRc, Timer, TimerMode, VecModel};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};
use tracing_subscriber::EnvFilter;
use zbus::{Proxy, connection::Builder, interface};

mod apps;

slint::include_modules!();

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = std::env::args().collect::<Vec<_>>();
    let windowed = args.iter().any(|arg| arg == "--windowed");
    let requested_view = requested_view(&args);

    if request_remote_view(requested_view)? {
        return Ok(());
    }

    let config = load_config()?;
    let apps = apps::discover_apps();
    let sessions = user_sessions();
    let favorite_ids = load_favorite_app_ids();
    let running_counts = Rc::new(RefCell::new(current_running_app_counts(&apps)));
    let featured_apps = featured_or_favorite_apps(&apps, &favorite_ids, 6);
    let recent_ids = load_recent_app_ids();
    let recent_app_entries = recent_apps(&apps, &recent_ids, 6);
    let recent_files = recent_files(3);
    let notifications = Arc::new(NotificationStore::default());
    notifications.push(Notification {
        title: "SeatShell ready".into(),
        body: "Notification center is available from the panel.".into(),
        urgency: NotificationUrgency::Low,
    });
    tracing::info!(count = apps.len(), "discovered launcher apps");

    let ui = AppWindow::new()?;
    ui.set_launcher_apps(launcher_model(
        &apps,
        &favorite_ids,
        None,
        &running_counts.borrow(),
    ));
    ui.set_featured_apps(launcher_model(
        &featured_apps,
        &favorite_ids,
        None,
        &running_counts.borrow(),
    ));
    ui.set_recent_apps(launcher_model(
        &recent_app_entries,
        &favorite_ids,
        None,
        &running_counts.borrow(),
    ));
    ui.set_active_apps(launcher_model(
        &active_apps(&apps, &running_counts.borrow(), 5),
        &favorite_ids,
        None,
        &running_counts.borrow(),
    ));
    ui.set_recent_files(recent_file_model(&recent_files));
    ui.set_panel_apps(launcher_model(
        &taskbar_apps(&apps, &favorite_ids, &running_counts.borrow(), 5),
        &favorite_ids,
        None,
        &running_counts.borrow(),
    ));
    ui.set_launcher_count(apps.len() as i32);
    let selected_session_action = initial_selected_session_action(&sessions);
    ui.set_user_sessions(session_model(&sessions, selected_session_action.as_deref()));
    ui.set_notifications(notification_model(&[]));
    ui.set_notification_count(0);

    ui.set_panel_height((config.panel.height as i32).clamp(34, 38));
    ui.set_accent(config.desktop.accent.into());
    ui.set_clock_text(clock_text().into());
    ui.set_status_text(shell_status_text(&sessions).into());
    ui.set_network_text(network_status_text().into());
    ui.set_power_text(power_status_text().into());
    ui.set_audio_text(audio_status_text().into());
    ui.set_overview_enabled(config.overview.enabled);
    ui.set_show_launcher(matches!(requested_view, ShellView::Launcher));
    ui.set_show_overview(matches!(requested_view, ShellView::Overview));
    ui.set_windowed(windowed);
    ui.set_panel_on_top(matches!(config.panel.position, PanelPosition::Top));
    ui.set_show_user_switcher(config.panel.show_user_switcher && config.overview.enabled);
    ui.set_theme_name(config.desktop.theme.into());
    ui.set_wallpaper_path(config.desktop.wallpaper.into());
    ui.set_app_count(apps.len() as i32);
    ui.set_session_count(sessions.len() as i32);
    ui.set_user_name(current_username().into());

    let weak = ui.as_weak();
    let clock_timer = Timer::default();
    clock_timer.start(TimerMode::Repeated, Duration::from_secs(30), move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_clock_text(clock_text().into());
            ui.set_network_text(network_status_text().into());
            ui.set_power_text(power_status_text().into());
            ui.set_audio_text(audio_status_text().into());
        }
    });

    let weak = ui.as_weak();
    let notifications_for_timer = Arc::clone(&notifications);
    let notification_revision = Rc::new(RefCell::new(0_u64));
    let notification_timer = Timer::default();
    notification_timer.start(TimerMode::Repeated, Duration::from_millis(350), move || {
        let latest_revision = notifications_for_timer.revision();
        if latest_revision == *notification_revision.borrow() {
            return;
        }

        *notification_revision.borrow_mut() = latest_revision;
        let snapshot = notifications_for_timer.snapshot();
        if let Some(ui) = weak.upgrade() {
            ui.set_notifications(notification_model(&snapshot));
            ui.set_notification_count(snapshot.len() as i32);
        }
    });

    let weak = ui.as_weak();
    let remote_commands = Arc::new(AtomicU8::new(ShellView::None as u8));
    spawn_shell_service(Arc::clone(&remote_commands), Arc::clone(&notifications))?;
    let remote_timer = Timer::default();
    remote_timer.start(TimerMode::Repeated, Duration::from_millis(120), move || {
        let command =
            ShellView::from_u8(remote_commands.swap(ShellView::None as u8, Ordering::SeqCst));
        if let Some(ui) = weak.upgrade() {
            match command {
                ShellView::Launcher => {
                    ui.set_show_launcher(true);
                    ui.set_show_overview(false);
                    ui.set_show_notifications(false);
                }
                ShellView::Overview => {
                    ui.set_show_overview(true);
                    ui.set_show_launcher(false);
                    ui.set_show_notifications(false);
                }
                ShellView::Notifications => {
                    ui.set_show_notifications(true);
                    ui.set_show_launcher(false);
                    ui.set_show_overview(false);
                    ui.set_show_command_surface(false);
                }
                ShellView::Desktop => {
                    ui.set_show_launcher(false);
                    ui.set_show_overview(false);
                    ui.set_show_notifications(false);
                }
                ShellView::None => {}
            }
        }
    });

    let weak = ui.as_weak();
    ui.on_show_desktop(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_show_launcher(false);
            ui.set_show_overview(false);
            ui.set_show_notifications(false);
        }
    });

    let weak = ui.as_weak();
    ui.on_toggle_launcher(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_show_launcher(!ui.get_show_launcher());
            ui.set_show_overview(false);
            ui.set_show_command_surface(false);
            ui.set_show_notifications(false);
        }
    });

    let weak = ui.as_weak();
    ui.on_toggle_overview(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_show_overview(!ui.get_show_overview());
            ui.set_show_launcher(false);
            ui.set_show_command_surface(false);
            ui.set_show_notifications(false);
        }
    });

    let weak = ui.as_weak();
    ui.on_toggle_command_surface(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_show_command_surface(!ui.get_show_command_surface());
            ui.set_show_launcher(false);
            ui.set_show_overview(false);
            ui.set_show_notifications(false);
        }
    });

    let weak = ui.as_weak();
    ui.on_toggle_notifications(move || {
        if let Some(ui) = weak.upgrade() {
            let next = !ui.get_show_notifications();
            ui.set_show_notifications(next);
            if next {
                ui.set_show_launcher(false);
                ui.set_show_overview(false);
                ui.set_show_command_surface(false);
            }
        }
    });

    {
        let notifications = Arc::clone(&notifications);
        ui.on_clear_notifications(move || {
            notifications.clear();
        });
    }

    {
        let notifications = Arc::clone(&notifications);
        ui.on_dismiss_notification(move |id| {
            notifications.dismiss(id as u32);
        });
    }

    {
        let notifications = Arc::clone(&notifications);
        ui.on_open_file(move |path| {
            if let Err(error) = open_path(path.as_str()) {
                tracing::warn!(%error, path = %path, "failed to open file");
                notifications.push(Notification {
                    title: "Open file failed".into(),
                    body: "Could not open the selected file.".into(),
                    urgency: NotificationUrgency::Critical,
                });
            }
        });
    }

    {
        let notifications = Arc::clone(&notifications);
        let weak = ui.as_weak();
        ui.on_toggle_network(move || match toggle_networking() {
            Ok(()) => {
                let label = network_status_text();
                notifications.push(Notification {
                    title: "Network updated".into(),
                    body: label.clone(),
                    urgency: NotificationUrgency::Normal,
                });
                if let Some(ui) = weak.upgrade() {
                    ui.set_network_text(label.into());
                }
            }
            Err(error) => {
                tracing::warn!(%error, "failed to toggle networking");
                notifications.push(Notification {
                    title: "Network control failed".into(),
                    body: "Could not toggle networking.".into(),
                    urgency: NotificationUrgency::Critical,
                });
            }
        });
    }

    {
        let notifications = Arc::clone(&notifications);
        let weak = ui.as_weak();
        ui.on_volume_down(move || {
            if let Err(error) = adjust_audio_volume(AudioStep::Down) {
                tracing::warn!(%error, "failed to lower audio volume");
                notifications.push(Notification {
                    title: "Audio control failed".into(),
                    body: "Could not lower volume.".into(),
                    urgency: NotificationUrgency::Critical,
                });
            }

            if let Some(ui) = weak.upgrade() {
                ui.set_audio_text(audio_status_text().into());
            }
        });
    }

    {
        let notifications = Arc::clone(&notifications);
        let weak = ui.as_weak();
        ui.on_volume_up(move || {
            if let Err(error) = adjust_audio_volume(AudioStep::Up) {
                tracing::warn!(%error, "failed to raise audio volume");
                notifications.push(Notification {
                    title: "Audio control failed".into(),
                    body: "Could not raise volume.".into(),
                    urgency: NotificationUrgency::Critical,
                });
            }

            if let Some(ui) = weak.upgrade() {
                ui.set_audio_text(audio_status_text().into());
            }
        });
    }

    {
        let notifications = Arc::clone(&notifications);
        let weak = ui.as_weak();
        ui.on_toggle_mute(move || {
            if let Err(error) = toggle_audio_mute() {
                tracing::warn!(%error, "failed to toggle audio mute");
                notifications.push(Notification {
                    title: "Audio control failed".into(),
                    body: "Could not toggle mute.".into(),
                    urgency: NotificationUrgency::Critical,
                });
            }

            if let Some(ui) = weak.upgrade() {
                ui.set_audio_text(audio_status_text().into());
            }
        });
    }

    let all_apps = Rc::new(apps);
    let all_sessions = Rc::new(sessions);
    let favorite_app_ids = Rc::new(RefCell::new(favorite_ids));
    let recent_app_ids = Rc::new(RefCell::new(recent_ids));
    let filtered_apps = Rc::new(RefCell::new(all_apps.as_ref().clone()));
    let selected_app_id = Rc::new(RefCell::new(initial_selected_app_id(
        filtered_apps.borrow().as_slice(),
    )));
    let selected_session_action = Rc::new(RefCell::new(selected_session_action));

    {
        let all_apps = Rc::clone(&all_apps);
        let filtered_apps = Rc::clone(&filtered_apps);
        let favorite_app_ids = Rc::clone(&favorite_app_ids);
        let recent_app_ids = Rc::clone(&recent_app_ids);
        let selected_app_id = Rc::clone(&selected_app_id);
        let running_counts = Rc::clone(&running_counts);
        let weak = ui.as_weak();
        let running_timer = Timer::default();
        running_timer.start(TimerMode::Repeated, Duration::from_secs(4), move || {
            let latest = current_running_app_counts(&all_apps);
            if latest == *running_counts.borrow() {
                return;
            }

            *running_counts.borrow_mut() = latest;
            if let Some(ui) = weak.upgrade() {
                refresh_app_models(
                    &ui,
                    &all_apps,
                    filtered_apps.borrow().as_slice(),
                    &favorite_app_ids.borrow(),
                    &recent_app_ids.borrow(),
                    selected_app_id.borrow().as_deref(),
                    &running_counts.borrow(),
                );
            }
        });
    }

    let weak = ui.as_weak();
    {
        let all_apps = Rc::clone(&all_apps);
        let filtered_apps = Rc::clone(&filtered_apps);
        let favorite_app_ids = Rc::clone(&favorite_app_ids);
        let selected_app_id = Rc::clone(&selected_app_id);
        let running_counts = Rc::clone(&running_counts);
        ui.on_launcher_search_changed(move |query| {
            let matches = filter_apps(&all_apps, query.as_str());
            *filtered_apps.borrow_mut() = matches.clone();
            *selected_app_id.borrow_mut() = initial_selected_app_id(&matches);

            if let Some(ui) = weak.upgrade() {
                ui.set_launcher_query(query);
                ui.set_launcher_count(matches.len() as i32);
                ui.set_launcher_apps(launcher_model(
                    &matches,
                    &favorite_app_ids.borrow(),
                    selected_app_id.borrow().as_deref(),
                    &running_counts.borrow(),
                ));
            }
        });
    }

    {
        let recent_model_apps = Rc::clone(&all_apps);
        let filtered_apps = Rc::clone(&filtered_apps);
        let recent_app_ids = Rc::clone(&recent_app_ids);
        let favorite_app_ids = Rc::clone(&favorite_app_ids);
        let selected_app_id = Rc::clone(&selected_app_id);
        let notifications = Arc::clone(&notifications);
        let running_counts = Rc::clone(&running_counts);
        let weak = ui.as_weak();
        ui.on_launcher_accept_search(move || {
            if let Some(app) = selected_app(
                filtered_apps.borrow().as_slice(),
                selected_app_id.borrow().as_deref(),
            ) {
                launch_app(app);
                record_recent_launch(app, &recent_app_ids);
                notifications.push(Notification {
                    title: format!("Opened {}", app.name),
                    body: app_detail(app),
                    urgency: NotificationUrgency::Low,
                });

                if let Some(ui) = weak.upgrade() {
                    ui.set_recent_apps(launcher_model(
                        &recent_apps(&recent_model_apps, &recent_app_ids.borrow(), 6),
                        &favorite_app_ids.borrow(),
                        None,
                        &running_counts.borrow(),
                    ));
                    ui.set_show_launcher(false);
                    ui.set_show_notifications(false);
                }
            }
        });
    }

    let launch_apps = Rc::clone(&all_apps);
    let launch_favorites = Rc::clone(&favorite_app_ids);
    let launch_recent_app_ids = Rc::clone(&recent_app_ids);
    let launch_notifications = Arc::clone(&notifications);
    let launch_running_counts = Rc::clone(&running_counts);
    let weak = ui.as_weak();
    ui.on_launch_app(move |app_id| {
        tracing::info!(app_id = %app_id, "launcher app requested");

        let Some(app) = launch_apps.iter().find(|app| app_id == app.id) else {
            tracing::warn!(app_id = %app_id, "launcher app id was not found");
            return;
        };

        launch_app(app);
        record_recent_launch(app, &launch_recent_app_ids);
        launch_notifications.push(Notification {
            title: format!("Opened {}", app.name),
            body: app_detail(app),
            urgency: NotificationUrgency::Low,
        });

        if let Some(ui) = weak.upgrade() {
            ui.set_recent_apps(launcher_model(
                &recent_apps(&launch_apps, &launch_recent_app_ids.borrow(), 6),
                &launch_favorites.borrow(),
                None,
                &launch_running_counts.borrow(),
            ));
            ui.set_show_launcher(false);
            ui.set_show_overview(false);
            ui.set_show_command_surface(false);
            ui.set_show_notifications(false);
        }
    });

    {
        let all_apps = Rc::clone(&all_apps);
        let filtered_apps = Rc::clone(&filtered_apps);
        let favorite_app_ids = Rc::clone(&favorite_app_ids);
        let selected_app_id = Rc::clone(&selected_app_id);
        let recent_app_ids = Rc::clone(&recent_app_ids);
        let running_counts = Rc::clone(&running_counts);
        let weak = ui.as_weak();
        ui.on_move_launcher_selection(move |step| {
            *selected_app_id.borrow_mut() = move_selected_app_id(
                filtered_apps.borrow().as_slice(),
                selected_app_id.borrow().as_deref(),
                step,
            );

            if let Some(ui) = weak.upgrade() {
                ui.set_launcher_apps(launcher_model(
                    filtered_apps.borrow().as_slice(),
                    &favorite_app_ids.borrow(),
                    selected_app_id.borrow().as_deref(),
                    &running_counts.borrow(),
                ));
                ui.set_featured_apps(launcher_model(
                    &featured_or_favorite_apps(&all_apps, &favorite_app_ids.borrow(), 6),
                    &favorite_app_ids.borrow(),
                    None,
                    &running_counts.borrow(),
                ));
                ui.set_recent_apps(launcher_model(
                    &recent_apps(&all_apps, &recent_app_ids.borrow(), 6),
                    &favorite_app_ids.borrow(),
                    None,
                    &running_counts.borrow(),
                ));
                ui.set_panel_apps(launcher_model(
                    &taskbar_apps(
                        &all_apps,
                        &favorite_app_ids.borrow(),
                        &running_counts.borrow(),
                        8,
                    ),
                    &favorite_app_ids.borrow(),
                    None,
                    &running_counts.borrow(),
                ));
            }
        });
    }

    {
        let all_apps = Rc::clone(&all_apps);
        let filtered_apps = Rc::clone(&filtered_apps);
        let favorite_app_ids = Rc::clone(&favorite_app_ids);
        let selected_app_id = Rc::clone(&selected_app_id);
        let recent_app_ids = Rc::clone(&recent_app_ids);
        let running_counts = Rc::clone(&running_counts);
        let weak = ui.as_weak();
        ui.on_toggle_favorite_app(move |app_id| {
            toggle_favorite_app_id(&app_id, &favorite_app_ids);

            if let Some(ui) = weak.upgrade() {
                ui.set_launcher_apps(launcher_model(
                    filtered_apps.borrow().as_slice(),
                    &favorite_app_ids.borrow(),
                    selected_app_id.borrow().as_deref(),
                    &running_counts.borrow(),
                ));
                ui.set_featured_apps(launcher_model(
                    &featured_or_favorite_apps(&all_apps, &favorite_app_ids.borrow(), 6),
                    &favorite_app_ids.borrow(),
                    None,
                    &running_counts.borrow(),
                ));
                ui.set_panel_apps(launcher_model(
                    &taskbar_apps(
                        &all_apps,
                        &favorite_app_ids.borrow(),
                        &running_counts.borrow(),
                        8,
                    ),
                    &favorite_app_ids.borrow(),
                    None,
                    &running_counts.borrow(),
                ));
                ui.set_recent_apps(launcher_model(
                    &recent_apps(&all_apps, &recent_app_ids.borrow(), 6),
                    &favorite_app_ids.borrow(),
                    None,
                    &running_counts.borrow(),
                ));
            }
        });
    }

    {
        let all_sessions = Rc::clone(&all_sessions);
        let selected_session_action = Rc::clone(&selected_session_action);
        let weak = ui.as_weak();
        ui.on_move_overview_selection(move |step| {
            *selected_session_action.borrow_mut() = move_selected_session_action(
                all_sessions.as_slice(),
                selected_session_action.borrow().as_deref(),
                step,
            );

            if let Some(ui) = weak.upgrade() {
                ui.set_user_sessions(session_model(
                    all_sessions.as_slice(),
                    selected_session_action.borrow().as_deref(),
                ));
            }
        });
    }

    {
        let selected_session_action = Rc::clone(&selected_session_action);
        let notifications = Arc::clone(&notifications);
        let weak = ui.as_weak();
        ui.on_activate_current_session(move || {
            let Some(action) = selected_session_action.borrow().clone() else {
                return;
            };

            match activate_session(&action) {
                Ok(()) => {
                    notifications.push(Notification {
                        title: "Session activated".into(),
                        body: action.clone(),
                        urgency: NotificationUrgency::Normal,
                    });
                    if let Some(ui) = weak.upgrade() {
                        ui.set_status_text(format!("activated {action}").into());
                        ui.set_show_overview(false);
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, action, "failed to activate session");
                    notifications.push(Notification {
                        title: "Session activation failed".into(),
                        body: action.clone(),
                        urgency: NotificationUrgency::Critical,
                    });
                    if let Some(ui) = weak.upgrade() {
                        ui.set_status_text(format!("activate failed for {action}").into());
                    }
                }
            }
        });
    }

    ui.run()?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellView {
    None = 0,
    Desktop = 1,
    Launcher = 2,
    Overview = 3,
    Notifications = 4,
}

impl ShellView {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Desktop,
            2 => Self::Launcher,
            3 => Self::Overview,
            4 => Self::Notifications,
            _ => Self::None,
        }
    }
}

struct ShellService {
    commands: Arc<AtomicU8>,
    notifications: Arc<NotificationStore>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StoredNotification {
    id: u32,
    title: String,
    body: String,
    urgency: NotificationUrgency,
}

impl StoredNotification {
    fn from_payload(id: u32, payload: Notification) -> Self {
        Self {
            id,
            title: payload.title,
            body: payload.body,
            urgency: payload.urgency,
        }
    }
}

#[derive(Default)]
struct NotificationStore {
    items: Mutex<Vec<StoredNotification>>,
    revision: std::sync::atomic::AtomicU64,
    next_id: std::sync::atomic::AtomicU32,
}

impl NotificationStore {
    fn push(&self, notification: Notification) -> u32 {
        let id = self
            .next_id
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        let mut items = self.items.lock().expect("notification store poisoned");
        items.insert(0, StoredNotification::from_payload(id, notification));
        items.truncate(32);
        self.revision.fetch_add(1, Ordering::SeqCst);
        id
    }

    fn dismiss(&self, id: u32) {
        let mut items = self.items.lock().expect("notification store poisoned");
        let original_len = items.len();
        items.retain(|notification| notification.id != id);
        if items.len() != original_len {
            self.revision.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn clear(&self) {
        let mut items = self.items.lock().expect("notification store poisoned");
        if items.is_empty() {
            return;
        }
        items.clear();
        self.revision.fetch_add(1, Ordering::SeqCst);
    }

    fn snapshot(&self) -> Vec<StoredNotification> {
        self.items
            .lock()
            .expect("notification store poisoned")
            .clone()
    }

    fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }
}

#[interface(name = "org.seatshell.Shell")]
impl ShellService {
    async fn show_desktop(&self) {
        self.commands
            .store(ShellView::Desktop as u8, Ordering::SeqCst);
    }

    async fn show_launcher(&self) {
        self.commands
            .store(ShellView::Launcher as u8, Ordering::SeqCst);
    }

    async fn show_overview(&self) {
        self.commands
            .store(ShellView::Overview as u8, Ordering::SeqCst);
    }

    async fn show_notifications(&self) {
        self.commands
            .store(ShellView::Notifications as u8, Ordering::SeqCst);
    }

    async fn post_notification(&self, title: &str, body: &str) {
        self.notifications.push(Notification {
            title: title.trim().to_string(),
            body: body.trim().to_string(),
            urgency: NotificationUrgency::Normal,
        });
    }

    async fn clear_notifications(&self) {
        self.notifications.clear();
    }
}

fn spawn_shell_service(
    commands: Arc<AtomicU8>,
    notifications: Arc<NotificationStore>,
) -> Result<()> {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::warn!(%error, "failed to build shell D-Bus runtime");
                return;
            }
        };

        runtime.block_on(async move {
            let service = ShellService {
                commands,
                notifications,
            };
            let connection = Builder::session()
                .and_then(|builder| builder.name(SHELL_BUS_NAME))
                .and_then(|builder| builder.serve_at(SHELL_OBJECT_PATH, service));

            match connection {
                Ok(builder) => match builder.build().await {
                    Ok(_connection) => {
                        tracing::info!(
                            bus_name = SHELL_BUS_NAME,
                            object_path = SHELL_OBJECT_PATH,
                            "SeatShell shell service registered"
                        );
                        std::future::pending::<()>().await;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "failed to build shell D-Bus service");
                    }
                },
                Err(error) => {
                    tracing::warn!(%error, "failed to configure shell D-Bus service");
                }
            }
        });
    });

    Ok(())
}

fn requested_view(args: &[String]) -> ShellView {
    if args.iter().any(|arg| arg == "--launcher") {
        ShellView::Launcher
    } else if args.iter().any(|arg| arg == "--overview") {
        ShellView::Overview
    } else if args.iter().any(|arg| arg == "--notifications") {
        ShellView::Notifications
    } else if args.iter().any(|arg| arg == "--desktop") {
        ShellView::Desktop
    } else {
        ShellView::None
    }
}

fn request_remote_view(view: ShellView) -> Result<bool> {
    if matches!(view, ShellView::None) {
        return Ok(false);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;

    runtime.block_on(async move {
        let connection = match zbus::Connection::session().await {
            Ok(connection) => connection,
            Err(_) => return Ok(false),
        };

        let proxy = match Proxy::new(
            &connection,
            SHELL_BUS_NAME,
            SHELL_OBJECT_PATH,
            shell::INTERFACE,
        )
        .await
        {
            Ok(proxy) => proxy,
            Err(_) => return Ok(false),
        };

        let method = match view {
            ShellView::Desktop => shell::SHOW_DESKTOP,
            ShellView::Launcher => shell::SHOW_LAUNCHER,
            ShellView::Notifications => shell::SHOW_NOTIFICATIONS,
            ShellView::Overview => shell::SHOW_OVERVIEW,
            ShellView::None => return Ok(false),
        };

        proxy.call_method(method, &()).await?;
        Ok(true)
    })
}

fn launcher_model(
    apps: &[apps::AppEntry],
    favorite_ids: &[String],
    selected_id: Option<&str>,
    running_counts: &HashMap<String, i32>,
) -> ModelRc<LauncherApp> {
    ModelRc::new(VecModel::from(
        apps.iter()
            .map(|app| LauncherApp {
                id: app.id.clone().into(),
                name: app.name.clone().into(),
                detail: app_detail(app).into(),
                icon_text: apps::app_icon_text(app).into(),
                icon: load_app_icon(app),
                selected: selected_id == Some(app.id.as_str()),
                favorite: favorite_ids
                    .iter()
                    .any(|favorite_id| favorite_id == &app.id),
                running_count: *running_counts.get(&app.id).unwrap_or(&0),
            })
            .collect::<Vec<_>>(),
    ))
}

fn load_app_icon(app: &apps::AppEntry) -> Image {
    apps::app_icon_path(app)
        .and_then(|path| Image::load_from_path(&path).ok())
        .unwrap_or_default()
}

fn notification_model(notifications: &[StoredNotification]) -> ModelRc<NotificationEntry> {
    ModelRc::new(VecModel::from(
        notifications
            .iter()
            .map(|notification| NotificationEntry {
                id: notification.id as i32,
                title: notification.title.clone().into(),
                body: notification.body.clone().into(),
                urgency: notification_urgency_text(&notification.urgency).into(),
            })
            .collect::<Vec<_>>(),
    ))
}

fn recent_file_model(files: &[RecentFileEntry]) -> ModelRc<DesktopFile> {
    ModelRc::new(VecModel::from(
        files.iter()
            .map(|file| DesktopFile {
                path: file.path.clone().into(),
                name: file.name.clone().into(),
                detail: file.detail.clone().into(),
            })
            .collect::<Vec<_>>(),
    ))
}

fn refresh_app_models(
    ui: &AppWindow,
    all_apps: &[apps::AppEntry],
    filtered_apps: &[apps::AppEntry],
    favorite_ids: &[String],
    recent_ids: &[String],
    selected_id: Option<&str>,
    running_counts: &HashMap<String, i32>,
) {
    ui.set_launcher_apps(launcher_model(
        filtered_apps,
        favorite_ids,
        selected_id,
        running_counts,
    ));
    ui.set_featured_apps(launcher_model(
        &featured_or_favorite_apps(all_apps, favorite_ids, 6),
        favorite_ids,
        None,
        running_counts,
    ));
    ui.set_recent_apps(launcher_model(
        &recent_apps(all_apps, recent_ids, 6),
        favorite_ids,
        None,
        running_counts,
    ));
    ui.set_active_apps(launcher_model(
        &active_apps(all_apps, running_counts, 5),
        favorite_ids,
        None,
        running_counts,
    ));
    ui.set_panel_apps(launcher_model(
        &taskbar_apps(all_apps, favorite_ids, running_counts, 5),
        favorite_ids,
        None,
        running_counts,
    ));
}

fn notification_urgency_text(urgency: &NotificationUrgency) -> &'static str {
    match urgency {
        NotificationUrgency::Low => "low",
        NotificationUrgency::Normal => "normal",
        NotificationUrgency::Critical => "critical",
    }
}

fn session_model(sessions: &[UserSession], selected_action: Option<&str>) -> ModelRc<UserSession> {
    ModelRc::new(VecModel::from(
        sessions
            .iter()
            .map(|session| UserSession {
                username: session.username.clone(),
                state: session.state.clone(),
                action: session.action.clone(),
                selected: selected_action == Some(session.action.as_str()),
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
                || app.detail.to_lowercase().contains(&query)
                || app
                    .categories
                    .iter()
                    .any(|category| category.to_lowercase().contains(&query))
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

fn open_path(path: &str) -> Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };

    std::process::Command::new(opener)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn {opener} for {path}"))?;

    Ok(())
}

fn clock_text() -> String {
    Local::now().format("%a %H:%M").to_string()
}

fn app_detail(app: &apps::AppEntry) -> String {
    if app.detail.trim().is_empty() {
        app.exec.clone()
    } else {
        app.detail.clone()
    }
}

fn shell_status_text(sessions: &[UserSession]) -> String {
    let seat = current_seat();
    let session_mode = if sessions.len() > 1 {
        "shared"
    } else {
        "local"
    };

    format!("{seat} {session_mode}")
}

fn activate_session(action: &str) -> Result<()> {
    let session_id = action.strip_prefix("session ").unwrap_or(action);
    let status = std::process::Command::new("loginctl")
        .args(["activate", session_id])
        .status()
        .context("failed to run loginctl activate")?;

    if !status.success() {
        bail!("loginctl activate {session_id} exited with {status}");
    }

    Ok(())
}

fn network_status_text() -> String {
    network_status_text_nmcli()
        .or_else(|error| {
            tracing::debug!(%error, "falling back to sysfs network status");
            Ok::<String, anyhow::Error>(network_status_text_sysfs())
        })
        .unwrap_or_else(|_| "NET ?".into())
}

fn network_status_text_nmcli() -> Result<String> {
    let networking_output = std::process::Command::new("nmcli")
        .arg("networking")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to run nmcli networking")?;

    if !networking_output.status.success() {
        bail!("nmcli networking exited with {}", networking_output.status);
    }

    let networking = String::from_utf8_lossy(&networking_output.stdout)
        .trim()
        .to_lowercase();
    if networking != "enabled" {
        return Ok("NET off".into());
    }

    let output = std::process::Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to run nmcli device")?;

    if !output.status.success() {
        bail!("nmcli device exited with {}", output.status);
    }

    parse_nmcli_device_status(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| anyhow::anyhow!("could not parse nmcli device status"))
}

fn network_status_text_sysfs() -> String {
    let entries = std::fs::read_dir("/sys/class/net");
    let Ok(entries) = entries else {
        return "NET ?".into();
    };

    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "lo" {
            continue;
        }

        let operstate_path = entry.path().join("operstate");
        let state = std::fs::read_to_string(operstate_path).unwrap_or_default();
        if state.trim() == "up" {
            return format!("NET {}", shorten_iface(&name));
        }
    }

    "NET off".into()
}

fn toggle_networking() -> Result<()> {
    let current = std::process::Command::new("nmcli")
        .arg("networking")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to run nmcli networking")?;

    if !current.status.success() {
        bail!("nmcli networking exited with {}", current.status);
    }

    let enabled = String::from_utf8_lossy(&current.stdout)
        .trim()
        .eq_ignore_ascii_case("enabled");
    let next = if enabled { "off" } else { "on" };

    let status = std::process::Command::new("nmcli")
        .args(["networking", next])
        .status()
        .context("failed to run nmcli networking toggle")?;

    if !status.success() {
        bail!("nmcli networking {next} exited with {}", status);
    }

    Ok(())
}

fn parse_nmcli_device_status(output: &str) -> Option<String> {
    let mut best: Option<(u8, String)> = None;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let parts = line.split(':').collect::<Vec<_>>();
        if parts.len() < 4 {
            continue;
        }

        let device = parts[0];
        let device_type = parts[1];
        let state = parts[2];
        let connection = parts[3];

        if device == "lo" || device_type == "loopback" {
            continue;
        }

        let priority = if state.starts_with("connected") {
            3
        } else if state == "disconnected" {
            2
        } else {
            1
        };
        let label = if state.starts_with("connected") {
            if connection.is_empty() || connection == "--" {
                format!("NET {}", shorten_iface(device))
            } else {
                format!("NET {}", shorten_connection(connection))
            }
        } else {
            format!("NET {}", state)
        };

        match &best {
            Some((best_priority, _)) if *best_priority >= priority => {}
            _ => best = Some((priority, label)),
        }
    }

    best.map(|(_, label)| label)
}

fn shorten_iface(name: &str) -> String {
    if name.len() <= 4 {
        name.to_uppercase()
    } else {
        name[..4].to_uppercase()
    }
}

fn shorten_connection(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.len() <= 10 {
        trimmed.to_string()
    } else {
        trimmed[..10].trim_end().to_string()
    }
}

fn power_status_text() -> String {
    let entries = std::fs::read_dir("/sys/class/power_supply");
    let Ok(entries) = entries else {
        return "AC".into();
    };

    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("BAT") {
            continue;
        }

        let capacity = std::fs::read_to_string(entry.path().join("capacity"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let status = std::fs::read_to_string(entry.path().join("status"))
            .unwrap_or_default()
            .trim()
            .to_string();

        if capacity.is_empty() {
            continue;
        }

        let prefix = if status.eq_ignore_ascii_case("charging") {
            "CHR"
        } else {
            "BAT"
        };

        return format!("{prefix} {capacity}%");
    }

    "AC".into()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AudioStep {
    Down,
    Up,
}

fn audio_status_text() -> String {
    audio_status_text_wpctl()
        .or_else(|error| {
            tracing::debug!(%error, "falling back to pactl audio status");
            audio_status_text_pactl()
        })
        .unwrap_or_else(|_| "VOL ?".into())
}

fn audio_status_text_wpctl() -> Result<String> {
    let output = std::process::Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to run wpctl get-volume")?;

    if !output.status.success() {
        bail!("wpctl get-volume exited with {}", output.status);
    }

    parse_wpctl_volume(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| anyhow::anyhow!("could not parse wpctl volume output"))
}

fn audio_status_text_pactl() -> Result<String> {
    let volume_output = std::process::Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to run pactl get-sink-volume")?;

    if !volume_output.status.success() {
        bail!("pactl get-sink-volume exited with {}", volume_output.status);
    }

    let mute_output = std::process::Command::new("pactl")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to run pactl get-sink-mute")?;

    if !mute_output.status.success() {
        bail!("pactl get-sink-mute exited with {}", mute_output.status);
    }

    let volume = parse_pactl_volume(&String::from_utf8_lossy(&volume_output.stdout))
        .ok_or_else(|| anyhow::anyhow!("could not parse pactl volume output"))?;
    let muted = parse_pactl_mute(&String::from_utf8_lossy(&mute_output.stdout))
        .ok_or_else(|| anyhow::anyhow!("could not parse pactl mute output"))?;

    Ok(format_audio_label(volume, muted))
}

fn adjust_audio_volume(step: AudioStep) -> Result<()> {
    adjust_audio_volume_wpctl(step).or_else(|error| {
        tracing::debug!(%error, "falling back to pactl volume control");
        adjust_audio_volume_pactl(step)
    })
}

fn adjust_audio_volume_wpctl(step: AudioStep) -> Result<()> {
    let amount = match step {
        AudioStep::Down => "5%-",
        AudioStep::Up => "5%+",
    };

    let status = std::process::Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", amount])
        .status()
        .context("failed to run wpctl set-volume")?;

    if !status.success() {
        bail!("wpctl set-volume exited with {}", status);
    }

    Ok(())
}

fn adjust_audio_volume_pactl(step: AudioStep) -> Result<()> {
    let amount = match step {
        AudioStep::Down => "-5%",
        AudioStep::Up => "+5%",
    };

    let status = std::process::Command::new("pactl")
        .args(["set-sink-volume", "@DEFAULT_SINK@", amount])
        .status()
        .context("failed to run pactl set-sink-volume")?;

    if !status.success() {
        bail!("pactl set-sink-volume exited with {}", status);
    }

    Ok(())
}

fn toggle_audio_mute() -> Result<()> {
    toggle_audio_mute_wpctl().or_else(|error| {
        tracing::debug!(%error, "falling back to pactl mute toggle");
        toggle_audio_mute_pactl()
    })
}

fn toggle_audio_mute_wpctl() -> Result<()> {
    let status = std::process::Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
        .status()
        .context("failed to run wpctl set-mute")?;

    if !status.success() {
        bail!("wpctl set-mute exited with {}", status);
    }

    Ok(())
}

fn toggle_audio_mute_pactl() -> Result<()> {
    let status = std::process::Command::new("pactl")
        .args(["set-sink-mute", "@DEFAULT_SINK@", "toggle"])
        .status()
        .context("failed to run pactl set-sink-mute")?;

    if !status.success() {
        bail!("pactl set-sink-mute exited with {}", status);
    }

    Ok(())
}

fn parse_wpctl_volume(output: &str) -> Option<String> {
    let volume = output
        .split_whitespace()
        .find_map(|token| token.parse::<f32>().ok())?;
    let muted = output.contains("[MUTED]");
    Some(format_audio_label((volume * 100.0).round() as i32, muted))
}

fn parse_pactl_volume(output: &str) -> Option<i32> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_suffix('%'))
        .and_then(|value| value.parse::<i32>().ok())
}

fn parse_pactl_mute(output: &str) -> Option<bool> {
    let value = output.split_once(':')?.1.trim();
    match value {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

fn format_audio_label(percent: i32, muted: bool) -> String {
    if muted {
        "MUTED".into()
    } else {
        format!("VOL {}%", percent.clamp(0, 150))
    }
}

fn current_username() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".into())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecentFileEntry {
    path: String,
    name: String,
    detail: String,
}

fn recent_files(limit: usize) -> Vec<RecentFileEntry> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };

    let roots = ["Desktop", "Documents", "Downloads"]
        .iter()
        .map(|dir| home.join(dir))
        .collect::<Vec<_>>();

    let mut entries = roots
        .iter()
        .flat_map(|root| files_in_dir(root))
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| right.0.cmp(&left.0));
    entries.truncate(limit);

    entries
        .into_iter()
        .map(|(_, path)| RecentFileEntry {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file")
                .to_string(),
            detail: recent_file_detail(&path),
            path: path.display().to_string(),
        })
        .collect()
}

fn files_in_dir(root: &Path) -> Vec<(std::time::SystemTime, PathBuf)> {
    let Ok(read_dir) = fs::read_dir(root) else {
        return Vec::new();
    };

    read_dir
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            if should_hide_recent_file(&path) {
                return None;
            }
            let modified = metadata.modified().ok()?;
            Some((modified, path))
        })
        .collect()
}

fn should_hide_recent_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };

    if name.starts_with('.') {
        return true;
    }

    matches!(
        name,
        ".DS_Store" | ".localized" | "Icon\r" | "Thumbs.db" | "desktop.ini"
    )
}

fn recent_file_detail(path: &Path) -> String {
    let folder = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Files");
    let kind = path
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.to_uppercase())
        .unwrap_or_else(|| "FILE".into());

    format!("{folder} • {kind}")
}

fn current_seat() -> String {
    std::env::var("XDG_SEAT").unwrap_or_else(|_| "seat0".into())
}

fn recent_apps(
    apps: &[apps::AppEntry],
    recent_ids: &[String],
    limit: usize,
) -> Vec<apps::AppEntry> {
    ordered_apps_from_ids(apps, recent_ids, limit)
}

fn featured_or_favorite_apps(
    apps: &[apps::AppEntry],
    favorite_ids: &[String],
    limit: usize,
) -> Vec<apps::AppEntry> {
    let mut ordered = ordered_apps_from_ids(apps, favorite_ids, limit);

    if ordered.len() >= limit {
        return ordered;
    }

    for app in apps::featured_apps(apps, limit) {
        if ordered.iter().any(|existing| existing.id == app.id) {
            continue;
        }

        ordered.push(app);
        if ordered.len() == limit {
            break;
        }
    }

    ordered
}

fn taskbar_apps(
    apps: &[apps::AppEntry],
    favorite_ids: &[String],
    running_counts: &HashMap<String, i32>,
    limit: usize,
) -> Vec<apps::AppEntry> {
    let mut ordered = ordered_apps_from_ids(apps, favorite_ids, limit);

    let mut running = apps
        .iter()
        .filter(|app| running_counts.get(&app.id).copied().unwrap_or_default() > 0)
        .cloned()
        .collect::<Vec<_>>();

    running.sort_by(|left, right| {
        running_counts
            .get(&right.id)
            .copied()
            .unwrap_or_default()
            .cmp(&running_counts.get(&left.id).copied().unwrap_or_default())
            .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    for app in running {
        if ordered.iter().any(|existing| existing.id == app.id) {
            continue;
        }

        ordered.push(app);
        if ordered.len() == limit {
            break;
        }
    }

    ordered
}

fn active_apps(
    apps: &[apps::AppEntry],
    running_counts: &HashMap<String, i32>,
    limit: usize,
) -> Vec<apps::AppEntry> {
    let mut running = apps
        .iter()
        .filter(|app| running_counts.get(&app.id).copied().unwrap_or_default() > 0)
        .cloned()
        .collect::<Vec<_>>();

    running.sort_by(|left, right| {
        running_counts
            .get(&right.id)
            .copied()
            .unwrap_or_default()
            .cmp(&running_counts.get(&left.id).copied().unwrap_or_default())
            .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    running.truncate(limit);
    running
}

fn ordered_apps_from_ids(
    apps: &[apps::AppEntry],
    ids: &[String],
    limit: usize,
) -> Vec<apps::AppEntry> {
    ids.iter()
        .filter_map(|id| apps.iter().find(|app| app.id == *id))
        .take(limit)
        .cloned()
        .collect()
}

fn current_running_app_counts(apps: &[apps::AppEntry]) -> HashMap<String, i32> {
    let processes = running_process_names().unwrap_or_default();
    running_counts_for_apps(apps, &processes)
}

fn running_process_names() -> Result<Vec<String>> {
    user_agent_running_processes().or_else(|error| {
        tracing::debug!(%error, "falling back to local process scan");
        local_running_processes()
    })
}

fn user_agent_running_processes() -> Result<Vec<String>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;

    runtime.block_on(async {
        let connection = zbus::Connection::session().await?;
        let uid = unsafe { libc::getuid() };
        let bus_name = format!("{}.u{uid}", seatshell_protocol::USER_AGENT_BUS_NAME_PREFIX);
        let proxy = Proxy::new(
            &connection,
            bus_name.as_str(),
            seatshell_protocol::USER_AGENT_OBJECT_PATH,
            seatshell_protocol::user_agent::INTERFACE,
        )
        .await?;

        let processes = tokio::time::timeout(
            Duration::from_millis(600),
            proxy.call::<_, _, Vec<String>>(seatshell_protocol::user_agent::LIST_RUNNING_APPS, &()),
        )
        .await??;

        Ok(processes)
    })
}

fn local_running_processes() -> Result<Vec<String>> {
    let uid = unsafe { libc::getuid() }.to_string();
    let output = std::process::Command::new("ps")
        .args(["-u", uid.as_str(), "-o", "comm="])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to run ps for running app scan")?;

    if !output.status.success() {
        bail!("ps exited with {}", output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| *line != "ps")
        .map(ToOwned::to_owned)
        .collect())
}

fn running_counts_for_apps(apps: &[apps::AppEntry], processes: &[String]) -> HashMap<String, i32> {
    let normalized_processes = processes
        .iter()
        .map(|process| process_name(process))
        .collect::<Vec<_>>();

    let mut counts = HashMap::new();
    for app in apps {
        let tokens = app_process_tokens(app);
        let running_count = normalized_processes
            .iter()
            .filter(|process| tokens.iter().any(|token| token == *process))
            .count() as i32;

        if running_count > 0 {
            counts.insert(app.id.clone(), running_count);
        }
    }

    counts
}

fn app_process_tokens(app: &apps::AppEntry) -> Vec<String> {
    let mut tokens = Vec::new();

    if let Some(program) = app.argv.first() {
        tokens.push(process_name(program));
    } else if let Some(program) =
        apps::split_command(&app.exec).and_then(|parts| parts.into_iter().next())
    {
        tokens.push(process_name(&program));
    }

    if !app.icon_name.is_empty() {
        let icon_token = process_name(&app.icon_name);
        if !tokens.iter().any(|token| token == &icon_token) {
            tokens.push(icon_token);
        }
    }

    tokens
}

fn process_name(program: &str) -> String {
    std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .to_lowercase()
}

fn load_recent_app_ids() -> Vec<String> {
    load_state_id_list("recent-apps.txt")
}

fn record_recent_launch(app: &apps::AppEntry, recent_app_ids: &Rc<RefCell<Vec<String>>>) {
    let mut ids = recent_app_ids.borrow_mut();
    ids.retain(|id| id != &app.id);
    ids.insert(0, app.id.clone());
    ids.truncate(12);
    persist_state_id_list("recent-apps.txt", &ids);
}

fn load_favorite_app_ids() -> Vec<String> {
    load_state_id_list("favorite-apps.txt")
}

fn toggle_favorite_app_id(app_id: &str, favorite_app_ids: &Rc<RefCell<Vec<String>>>) {
    let mut ids = favorite_app_ids.borrow_mut();

    if let Some(index) = ids.iter().position(|id| id == app_id) {
        ids.remove(index);
    } else {
        ids.insert(0, app_id.to_string());
    }

    ids.truncate(12);
    persist_state_id_list("favorite-apps.txt", &ids);
}

fn initial_selected_app_id(apps: &[apps::AppEntry]) -> Option<String> {
    apps.first().map(|app| app.id.clone())
}

fn move_selected_app_id(
    apps: &[apps::AppEntry],
    current_id: Option<&str>,
    step: i32,
) -> Option<String> {
    if apps.is_empty() {
        return None;
    }

    let current_index = current_id
        .and_then(|id| apps.iter().position(|app| app.id == id))
        .unwrap_or(0);
    let next_index = (current_index as i32 + step).clamp(0, apps.len() as i32 - 1) as usize;
    Some(apps[next_index].id.clone())
}

fn selected_app<'a>(
    apps: &'a [apps::AppEntry],
    selected_id: Option<&str>,
) -> Option<&'a apps::AppEntry> {
    selected_id
        .and_then(|id| apps.iter().find(|app| app.id == id))
        .or_else(|| apps.first())
}

fn initial_selected_session_action(sessions: &[UserSession]) -> Option<String> {
    sessions.first().map(|session| session.action.to_string())
}

fn move_selected_session_action(
    sessions: &[UserSession],
    current_action: Option<&str>,
    step: i32,
) -> Option<String> {
    if sessions.is_empty() {
        return None;
    }

    let current_index = current_action
        .and_then(|action| {
            sessions
                .iter()
                .position(|session| session.action.as_str() == action)
        })
        .unwrap_or(0);
    let next_index = (current_index as i32 + step).clamp(0, sessions.len() as i32 - 1) as usize;
    Some(sessions[next_index].action.to_string())
}

fn load_state_id_list(file_name: &str) -> Vec<String> {
    let Some(path) = state_file_path(file_name) else {
        return Vec::new();
    };

    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn persist_state_id_list(file_name: &str, ids: &[String]) {
    let Some(path) = state_file_path(file_name) else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let _ = std::fs::write(path, ids.join("\n"));
}

fn state_file_path(file_name: &str) -> Option<std::path::PathBuf> {
    if let Some(state_dir) = std::env::var_os("SEATSHELL_STATE_DIR") {
        return Some(std::path::PathBuf::from(state_dir).join(file_name));
    }

    std::env::var_os("HOME").map(|home| {
        std::path::PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("seatshell")
            .join(file_name)
    })
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
        selected: false,
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
        selected: false,
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
                detail: "Open a terminal".into(),
                icon_name: "utilities-terminal".into(),
                categories: vec!["TerminalEmulator".into()],
            },
            apps::AppEntry {
                id: "org.example.Files.desktop".into(),
                name: "Files".into(),
                exec: "dolphin".into(),
                argv: vec!["dolphin".into()],
                detail: "Browse files".into(),
                icon_name: "system-file-manager".into(),
                categories: vec!["FileManager".into()],
            },
        ];

        assert_eq!(filter_apps(&apps, "").len(), 2);
        assert_eq!(filter_apps(&apps, "term")[0].name, "Terminal");
        assert_eq!(filter_apps(&apps, "dolphin")[0].name, "Files");
        assert_eq!(filter_apps(&apps, "browse")[0].name, "Files");
        assert!(filter_apps(&apps, "missing").is_empty());
    }

    #[test]
    fn recent_apps_follow_saved_order() {
        let apps = vec![
            apps::AppEntry {
                id: "terminal.desktop".into(),
                name: "Terminal".into(),
                exec: "konsole".into(),
                argv: vec!["konsole".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
            apps::AppEntry {
                id: "files.desktop".into(),
                name: "Files".into(),
                exec: "dolphin".into(),
                argv: vec!["dolphin".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
        ];

        let ordered = recent_apps(
            &apps,
            &["files.desktop".into(), "terminal.desktop".into()],
            6,
        );
        assert_eq!(ordered[0].name, "Files");
        assert_eq!(ordered[1].name, "Terminal");
    }

    #[test]
    fn favorites_are_used_before_fallback_apps() {
        let apps = vec![
            apps::AppEntry {
                id: "terminal.desktop".into(),
                name: "Terminal".into(),
                exec: "konsole".into(),
                argv: vec!["konsole".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
            apps::AppEntry {
                id: "files.desktop".into(),
                name: "Files".into(),
                exec: "dolphin".into(),
                argv: vec!["dolphin".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
            apps::AppEntry {
                id: "browser.desktop".into(),
                name: "Browser".into(),
                exec: "firefox".into(),
                argv: vec!["firefox".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
        ];

        let featured = featured_or_favorite_apps(
            &apps,
            &["browser.desktop".into(), "files.desktop".into()],
            3,
        );

        assert_eq!(featured[0].name, "Browser");
        assert_eq!(featured[1].name, "Files");
        assert_eq!(featured[2].name, "Terminal");
    }

    #[test]
    fn selection_moves_within_bounds() {
        let apps = vec![
            apps::AppEntry {
                id: "first.desktop".into(),
                name: "First".into(),
                exec: "first".into(),
                argv: vec!["first".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
            apps::AppEntry {
                id: "second.desktop".into(),
                name: "Second".into(),
                exec: "second".into(),
                argv: vec!["second".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
        ];

        assert_eq!(
            move_selected_app_id(&apps, Some("first.desktop"), -1).as_deref(),
            Some("first.desktop")
        );
        assert_eq!(
            move_selected_app_id(&apps, Some("first.desktop"), 1).as_deref(),
            Some("second.desktop")
        );
        assert_eq!(
            move_selected_app_id(&apps, Some("second.desktop"), 1).as_deref(),
            Some("second.desktop")
        );
    }

    #[test]
    fn overview_selection_moves_within_bounds() {
        let sessions = vec![
            UserSession {
                username: "alice".into(),
                state: "active".into(),
                action: "session a".into(),
                selected: false,
            },
            UserSession {
                username: "bob".into(),
                state: "locked".into(),
                action: "session b".into(),
                selected: false,
            },
        ];

        assert_eq!(
            move_selected_session_action(&sessions, Some("session a"), -1).as_deref(),
            Some("session a")
        );
        assert_eq!(
            move_selected_session_action(&sessions, Some("session a"), 1).as_deref(),
            Some("session b")
        );
        assert_eq!(
            move_selected_session_action(&sessions, Some("session b"), 1).as_deref(),
            Some("session b")
        );
    }

    #[test]
    fn notification_store_orders_newest_first_and_trims() {
        let store = NotificationStore::default();

        for index in 0..40 {
            store.push(Notification {
                title: format!("n{index}"),
                body: String::new(),
                urgency: NotificationUrgency::Normal,
            });
        }

        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 32);
        assert_eq!(snapshot[0].title, "n39");
        assert_eq!(
            snapshot.last().map(|entry| entry.title.as_str()),
            Some("n8")
        );
    }

    #[test]
    fn notification_store_dismiss_and_clear_update_contents() {
        let store = NotificationStore::default();
        let first = store.push(Notification {
            title: "one".into(),
            body: "body".into(),
            urgency: NotificationUrgency::Normal,
        });
        let second = store.push(Notification {
            title: "two".into(),
            body: String::new(),
            urgency: NotificationUrgency::Critical,
        });

        store.dismiss(first);
        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].id, second);

        store.clear();
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn running_counts_match_app_process_names() {
        let apps = vec![
            apps::AppEntry {
                id: "terminal.desktop".into(),
                name: "Terminal".into(),
                exec: "/usr/bin/konsole".into(),
                argv: vec!["/usr/bin/konsole".into()],
                detail: String::new(),
                icon_name: "utilities-terminal".into(),
                categories: vec![],
            },
            apps::AppEntry {
                id: "browser.desktop".into(),
                name: "Browser".into(),
                exec: "firefox-esr".into(),
                argv: vec!["firefox-esr".into()],
                detail: String::new(),
                icon_name: "firefox-esr".into(),
                categories: vec![],
            },
        ];

        let counts = running_counts_for_apps(
            &apps,
            &["konsole".into(), "firefox-esr".into(), "firefox-esr".into()],
        );

        assert_eq!(counts.get("terminal.desktop"), Some(&1));
        assert_eq!(counts.get("browser.desktop"), Some(&2));
    }

    #[test]
    fn taskbar_apps_include_favorites_then_running_apps() {
        let apps = vec![
            apps::AppEntry {
                id: "terminal.desktop".into(),
                name: "Terminal".into(),
                exec: "konsole".into(),
                argv: vec!["konsole".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
            apps::AppEntry {
                id: "browser.desktop".into(),
                name: "Browser".into(),
                exec: "firefox-esr".into(),
                argv: vec!["firefox-esr".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
            apps::AppEntry {
                id: "files.desktop".into(),
                name: "Files".into(),
                exec: "dolphin".into(),
                argv: vec!["dolphin".into()],
                detail: String::new(),
                icon_name: String::new(),
                categories: vec![],
            },
        ];

        let counts = HashMap::from([("browser.desktop".into(), 2), ("files.desktop".into(), 1)]);

        let ordered = taskbar_apps(&apps, &["terminal.desktop".into()], &counts, 6);
        let ids = ordered.into_iter().map(|app| app.id).collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "terminal.desktop".to_string(),
                "browser.desktop".to_string(),
                "files.desktop".to_string()
            ]
        );
    }

    #[test]
    fn parse_wpctl_output_detects_muted_and_percent() {
        assert_eq!(
            parse_wpctl_volume("Volume: 0.42"),
            Some("VOL 42%".to_string())
        );
        assert_eq!(
            parse_wpctl_volume("Volume: 0.00 [MUTED]"),
            Some("MUTED".to_string())
        );
    }

    #[test]
    fn parse_pactl_output_detects_volume_and_mute() {
        assert_eq!(
            parse_pactl_volume(
                "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 32768 /  50% / -18.06 dB"
            ),
            Some(50)
        );
        assert_eq!(parse_pactl_mute("Mute: yes"), Some(true));
        assert_eq!(parse_pactl_mute("Mute: no"), Some(false));
    }

    #[test]
    fn parse_nmcli_device_status_prefers_connected_non_loopback_device() {
        let output = "\
enp0s1:ethernet:connected:Wired connection 1
tailscale0:tun:connected (externally):tailscale0
lo:loopback:connected (externally):lo
";
        assert_eq!(
            parse_nmcli_device_status(output),
            Some("NET Wired conn".to_string())
        );
    }

    #[test]
    fn parse_nmcli_device_status_handles_disconnected_state() {
        let output = "wlp0s20f3:wifi:disconnected:--\n";
        assert_eq!(
            parse_nmcli_device_status(output),
            Some("NET disconnected".to_string())
        );
    }
}
