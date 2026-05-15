use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub urgency: NotificationUrgency,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}
