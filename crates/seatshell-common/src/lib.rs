use serde::{Deserialize, Serialize};
use std::fmt;

pub mod config;
pub mod session;

pub use config::SeatShellConfig;
pub use session::{SessionInfo, SessionState, UserInfo};

#[derive(Debug, thiserror::Error)]
pub enum SeatShellError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("session error: {0}")]
    Session(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopFileId(String);

impl DesktopFileId {
    pub fn new(value: impl Into<String>) -> Result<Self, SeatShellError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(SeatShellError::Config(
                "desktop file id cannot be empty".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DesktopFileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
