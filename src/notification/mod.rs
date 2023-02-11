use std::fmt;

use chrono::{DateTime, Utc};
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use integrations::github::GithubNotification;

use crate::task::{Task, TaskId};

pub mod integrations;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct Notification {
    pub id: NotificationId,
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
    pub task_id: Option<TaskId>,
    pub task_source_id: Option<String>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct NotificationWithTask {
    pub id: NotificationId,
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
    pub task: Option<Task>,
    pub task_source_id: Option<String>,
}

impl From<NotificationWithTask> for Notification {
    fn from(notification: NotificationWithTask) -> Self {
        (&notification).into()
    }
}

impl From<&NotificationWithTask> for Notification {
    fn from(notification: &NotificationWithTask) -> Self {
        Notification {
            id: notification.id,
            title: notification.title.clone(),
            source_id: notification.source_id.clone(),
            source_html_url: notification.source_html_url.clone(),
            status: notification.status,
            metadata: notification.metadata.clone(),
            updated_at: notification.updated_at,
            last_read_at: notification.last_read_at,
            snoozed_until: notification.snoozed_until,
            task_id: notification.task.as_ref().map(|task| task.id),
            task_source_id: notification.task_source_id.clone(),
        }
    }
}

impl NotificationWithTask {
    pub fn build(notification: &Notification, task: Option<Task>) -> Self {
        NotificationWithTask {
            id: notification.id,
            title: notification.title.clone(),
            source_id: notification.source_id.clone(),
            source_html_url: notification.source_html_url.clone(),
            status: notification.status,
            metadata: notification.metadata.clone(),
            updated_at: notification.updated_at,
            last_read_at: notification.last_read_at,
            snoozed_until: notification.snoozed_until,
            task,
            task_source_id: notification.task_source_id.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum NotificationMetadata {
    Github(GithubNotification),
    Todoist,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct NotificationId(pub Uuid);

impl fmt::Display for NotificationId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for NotificationId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<NotificationId> for Uuid {
    fn from(id: NotificationId) -> Self {
        id.0
    }
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

impl NotificationWithTask {
    pub fn is_built_from_task(&self) -> bool {
        matches!(self.metadata, NotificationMetadata::Todoist)
    }
}
