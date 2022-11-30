#[macro_use]
extern crate macro_attr;

#[macro_use]
extern crate enum_derive;

use chrono::{DateTime, Utc};
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use integrations::{github::GithubNotification, todoist::TodoistTask};

pub mod integrations;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct Notification {
    pub id: Uuid,
    pub title: String,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub source_html_url: Option<Uri>,
    pub status: NotificationStatus,
    pub metadata: NotificationMetadata,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum NotificationMetadata {
    Github(GithubNotification),
    Todoist(TodoistTask),
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationStatus {
        Unread,
        Read,
        Deleted,
        Unsubscribed,
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct NotificationPatch {
    pub status: Option<NotificationStatus>,
    pub snoozed_until: Option<DateTime<Utc>>,
}
