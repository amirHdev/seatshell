pub const ADMIN_BUS_NAME: &str = "org.seatshell.Admin";
pub const ADMIN_OBJECT_PATH: &str = "/org/seatshell/Admin";
pub const USER_AGENT_BUS_NAME_PREFIX: &str = "org.seatshell.UserAgent";
pub const USER_AGENT_OBJECT_PATH: &str = "/org/seatshell/UserAgent";
pub const SHELL_BUS_NAME: &str = "org.seatshell.Shell";
pub const SHELL_OBJECT_PATH: &str = "/org/seatshell/Shell";
pub const SETTINGS_BUS_NAME: &str = "org.seatshell.Settings";

pub mod admin {
    pub const INTERFACE: &str = "org.seatshell.Admin";
    pub const LIST_USERS: &str = "ListUsers";
    pub const LIST_SESSIONS: &str = "ListSessions";
    pub const GET_POLICY_GROUP: &str = "GetPolicyGroup";
    pub const OPEN_APP_AS_USER: &str = "OpenAppAsUser";
    pub const LOCK_SESSION: &str = "LockSession";
    pub const LOGOUT_SESSION: &str = "LogoutSession";
    pub const SEND_MESSAGE: &str = "SendMessage";
    pub const GET_SESSION_STATE: &str = "GetSessionState";
    pub const REQUEST_PREVIEW: &str = "RequestPreview";
}

pub mod user_agent {
    pub const INTERFACE: &str = "org.seatshell.UserAgent";
    pub const LAUNCH_DESKTOP_FILE: &str = "LaunchDesktopFile";
    pub const LAUNCH_COMMAND: &str = "LaunchCommand";
    pub const LIST_RUNNING_APPS: &str = "ListRunningApps";
    pub const GET_SESSION_INFO: &str = "GetSessionInfo";
    pub const SHOW_ADMIN_NOTICE: &str = "ShowAdminNotice";
    pub const START_PREVIEW_STREAM: &str = "StartPreviewStream";
    pub const STOP_PREVIEW_STREAM: &str = "StopPreviewStream";
}

pub mod shell {
    pub const INTERFACE: &str = "org.seatshell.Shell";
    pub const SHOW_DESKTOP: &str = "ShowDesktop";
    pub const SHOW_LAUNCHER: &str = "ShowLauncher";
    pub const SHOW_NOTIFICATIONS: &str = "ShowNotifications";
    pub const SHOW_OVERVIEW: &str = "ShowOverview";
    pub const POST_NOTIFICATION: &str = "PostNotification";
    pub const CLEAR_NOTIFICATIONS: &str = "ClearNotifications";
}
