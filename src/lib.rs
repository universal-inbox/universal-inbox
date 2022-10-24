#[macro_use]
extern crate macro_attr;

#[macro_use]
extern crate enum_derive;

use chrono::{DateTime, Utc};
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use integrations::github::GithubNotification;

pub mod integrations;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct Notification {
    pub id: Uuid,
    pub title: String,
    pub kind: NotificationKind,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub source_html_url: Option<Uri>,
    pub status: NotificationStatus,
    pub metadata: GithubNotification,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationKind {
        Github,
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationStatus {
        Unread,
        Read,
        Done,
        Unsubscribed,
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NotificationPatch {
    pub status: Option<NotificationStatus>,
}
