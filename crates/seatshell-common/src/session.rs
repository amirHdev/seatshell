use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UserInfo {
    pub uid: u32,
    pub username: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub uid: u32,
    pub username: String,
    pub seat: String,
    pub state: SessionState,
    pub locked: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Active,
    Online,
    Closing,
    Inactive,
    Unknown,
}
