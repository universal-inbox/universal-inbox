use std::fmt;

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use integrations::github::GithubNotification;

use crate::{
    integration_connection::IntegrationProvider,
    task::{Task, TaskId},
    user::UserId,
};

pub mod integrations;
pub mod service;

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
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
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
    pub user_id: UserId,
    pub task: Option<Task>,
}

pub trait IntoNotification {
    fn into_notification(self, user_id: UserId) -> Notification;
}

impl IntoNotification for NotificationWithTask {
    fn into_notification(self, user_id: UserId) -> Notification {
        Notification {
            id: self.id,
            title: self.title.clone(),
            source_id: self.source_id.clone(),
            source_html_url: self.source_html_url.clone(),
            status: self.status,
            metadata: self.metadata.clone(),
            updated_at: self.updated_at,
            last_read_at: self.last_read_at,
            snoozed_until: self.snoozed_until,
            user_id,
            task_id: self.task.as_ref().map(|task| task.id),
        }
    }
}

impl From<NotificationWithTask> for Notification {
    fn from(notification: NotificationWithTask) -> Self {
        let user_id = notification.user_id;
        notification.into_notification(user_id)
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
            user_id: notification.user_id,
            task,
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

impl NotificationWithTask {
    pub fn is_built_from_task(&self) -> bool {
        matches!(self.metadata, NotificationMetadata::Todoist)
    }
}

macro_attr! {
    // Synchronization sources for notifications
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationSyncSourceKind {
        Github
    }
}

macro_attr! {
    // notification sources, either direct or from tasks
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationSourceKind {
        Github,
        Todoist
    }
}

pub trait NotificationSource: IntegrationProvider {
    fn get_notification_source_kind(&self) -> NotificationSourceKind;
}
