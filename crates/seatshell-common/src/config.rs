use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct SeatShellConfig {
    pub desktop: DesktopConfig,
    pub panel: PanelConfig,
    pub overview: OverviewConfig,
    pub launcher: LauncherConfig,
    pub admin: AdminConfig,
    pub control: ControlConfig,
    pub privacy: PrivacyConfig,
    pub preview: PreviewConfig,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct DesktopConfig {
    pub theme: String,
    pub accent: String,
    pub wallpaper: String,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            accent: "#4a90e2".into(),
            wallpaper: "/usr/share/seatshell/wallpapers/default.jpg".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct PanelConfig {
    pub position: PanelPosition,
    pub height: u32,
    pub show_user_switcher: bool,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            position: PanelPosition::Bottom,
            height: 42,
            show_user_switcher: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelPosition {
    Top,
    #[default]
    Bottom,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct OverviewConfig {
    pub enabled: bool,
    pub shortcut: String,
    pub style: OverviewStyle,
    pub show_current_session: bool,
    pub show_inactive_users: bool,
    pub show_live_previews: bool,
    pub blur_locked_sessions: bool,
}

impl Default for OverviewConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            shortcut: "Super+Tab".into(),
            style: OverviewStyle::Grid,
            show_current_session: true,
            show_inactive_users: true,
            show_live_previews: false,
            blur_locked_sessions: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewStyle {
    #[default]
    Grid,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct LauncherConfig {
    pub terminal: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            terminal: "alacritty".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct AdminConfig {
    pub allowed_group: String,
    pub require_reauth: bool,
    pub log_actions: bool,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            allowed_group: "wheel".into(),
            require_reauth: true,
            log_actions: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct ControlConfig {
    pub allow_open_app_as_user: bool,
    pub allow_logout_user: bool,
    pub allow_lock_user: bool,
    pub allow_input_control: bool,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            allow_open_app_as_user: true,
            allow_logout_user: true,
            allow_lock_user: true,
            allow_input_control: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct PrivacyConfig {
    pub notify_user_on_admin_action: bool,
    pub require_consent_for_live_preview: bool,
    pub allow_preview_of_locked_sessions: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            notify_user_on_admin_action: true,
            require_consent_for_live_preview: true,
            allow_preview_of_locked_sessions: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct PreviewConfig {
    pub backend: PreviewBackend,
    pub blur_locked_sessions: bool,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            backend: PreviewBackend::None,
            blur_locked_sessions: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewBackend {
    #[default]
    None,
    PortalPipewire,
    CompositorNative,
}
