use std::fmt;

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use strum::{Display, EnumIter, EnumString};
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    task::{Task, TaskId},
    third_party::item::{ThirdPartyItem, ThirdPartyItemSourceKind},
    user::UserId,
    HasHtmlUrl,
};

pub mod service;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Notification {
    pub id: NotificationId,
    pub title: String,
    pub status: NotificationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub kind: NotificationSourceKind,
    pub source_item: ThirdPartyItem,
}

impl PartialEq for Notification {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.status == other.status
            && self.last_read_at == other.last_read_at
            && self.user_id == other.user_id
            && self.task_id == other.task_id
            && self.kind == other.kind
            && self.source_item.id == other.source_item.id
    }
}

impl HasHtmlUrl for Notification {
    fn get_html_url(&self) -> Url {
        self.source_item.get_html_url()
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotificationWithTask {
    pub id: NotificationId,
    pub title: String,
    pub status: NotificationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub user_id: UserId,
    pub task: Option<Task>,
    pub kind: NotificationSourceKind,
    pub source_item: ThirdPartyItem,
}

impl PartialEq for NotificationWithTask {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.status == other.status
            && self.last_read_at == other.last_read_at
            && self.user_id == other.user_id
            && self.task.as_ref().map(|t| t.id) == other.task.as_ref().map(|t| t.id)
            && self.kind == other.kind
            && self.source_item.id == other.source_item.id
    }
}

impl HasHtmlUrl for NotificationWithTask {
    fn get_html_url(&self) -> Url {
        match &self {
            NotificationWithTask {
                kind: NotificationSourceKind::Todoist,
                task: Some(task),
                ..
            } => task.get_html_url(),
            _ => Notification::from(self.clone()).get_html_url(),
        }
    }
}

impl From<NotificationWithTask> for Notification {
    fn from(notification: NotificationWithTask) -> Self {
        notification.into_notification()
    }
}

impl NotificationWithTask {
    pub fn build(notification: &Notification, task: Option<Task>) -> Self {
        NotificationWithTask {
            id: notification.id,
            title: notification.title.clone(),
            status: notification.status,
            created_at: notification.created_at,
            updated_at: notification.updated_at,
            last_read_at: notification.last_read_at,
            snoozed_until: notification.snoozed_until,
            user_id: notification.user_id,
            source_item: notification.source_item.clone(),
            kind: notification.kind,
            task,
        }
    }

    pub fn is_built_from_task(&self) -> bool {
        matches!(self.kind, NotificationSourceKind::Todoist)
    }

    pub fn into_notification(self) -> Notification {
        Notification {
            id: self.id,
            title: self.title.clone(),
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_read_at: self.last_read_at,
            snoozed_until: self.snoozed_until,
            user_id: self.user_id,
            task_id: self.task.as_ref().map(|task| task.id),
            kind: self.kind,
            source_item: self.source_item,
        }
    }
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

macro_attr! {
    // tag: New notification integration
    // Synchronization sources for notifications
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationSyncSourceKind {
        Github,
        Linear,
        GoogleMail
    }
}

macro_attr! {
    // tag: New notification integration
    // notification sources, either direct or from tasks
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!, EnumIter)]
    pub enum NotificationSourceKind {
        Github,
        Todoist,
        Linear,
        GoogleMail,
        GoogleCalendar,
        Slack
    }
}

impl TryFrom<ThirdPartyItemSourceKind> for NotificationSourceKind {
    type Error = anyhow::Error;

    fn try_from(source_kind: ThirdPartyItemSourceKind) -> Result<Self, Self::Error> {
        match source_kind {
            ThirdPartyItemSourceKind::Todoist => Ok(Self::Todoist),
            ThirdPartyItemSourceKind::GithubNotification => Ok(Self::Github),
            ThirdPartyItemSourceKind::LinearNotification => Ok(Self::Linear),
            ThirdPartyItemSourceKind::GoogleMailThread => Ok(Self::GoogleMail),
            ThirdPartyItemSourceKind::SlackReaction | ThirdPartyItemSourceKind::SlackStar => {
                Ok(Self::Slack)
            }
            _ => Err(anyhow!(
                "ThirdPartyItemSourceKind {source_kind} is not a valid NotificationSourceKind"
            )),
        }
    }
}

impl TryFrom<IntegrationProviderKind> for NotificationSyncSourceKind {
    type Error = anyhow::Error;

    // tag: New notification integration
    fn try_from(provider_kind: IntegrationProviderKind) -> Result<Self, Self::Error> {
        match provider_kind {
            IntegrationProviderKind::Github => Ok(Self::Github),
            IntegrationProviderKind::Linear => Ok(Self::Linear),
            IntegrationProviderKind::GoogleMail => Ok(Self::GoogleMail),
            _ => Err(anyhow!(
                "IntegrationProviderKind {provider_kind} is not a valid NotificationSyncSourceKind"
            )),
        }
    }
}

impl From<NotificationSyncSourceKind> for IntegrationProviderKind {
    // tag: New notification integration
    fn from(sync_source_kind: NotificationSyncSourceKind) -> Self {
        match sync_source_kind {
            NotificationSyncSourceKind::Github => IntegrationProviderKind::Github,
            NotificationSyncSourceKind::Linear => IntegrationProviderKind::Linear,
            NotificationSyncSourceKind::GoogleMail => IntegrationProviderKind::GoogleMail,
        }
    }
}

impl TryFrom<IntegrationProviderKind> for NotificationSourceKind {
    type Error = ();

    // tag: New notification integration
    fn try_from(provider_kind: IntegrationProviderKind) -> Result<Self, Self::Error> {
        match provider_kind {
            IntegrationProviderKind::Github => Ok(Self::Github),
            IntegrationProviderKind::Linear => Ok(Self::Linear),
            IntegrationProviderKind::GoogleMail => Ok(Self::GoogleMail),
            IntegrationProviderKind::GoogleCalendar => Ok(Self::GoogleCalendar),
            IntegrationProviderKind::Todoist => Ok(Self::Todoist),
            IntegrationProviderKind::Slack => Ok(Self::Slack),
            _ => Err(()),
        }
    }
}

impl From<NotificationSourceKind> for IntegrationProviderKind {
    // tag: New notification integration
    fn from(kind: NotificationSourceKind) -> Self {
        match kind {
            NotificationSourceKind::Github => Self::Github,
            NotificationSourceKind::Linear => Self::Linear,
            NotificationSourceKind::GoogleMail => Self::GoogleMail,
            NotificationSourceKind::GoogleCalendar => Self::GoogleCalendar,
            NotificationSourceKind::Todoist => Self::Todoist,
            NotificationSourceKind::Slack => Self::Slack,
        }
    }
}

pub trait NotificationSource: IntegrationProviderSource {
    fn get_notification_source_kind(&self) -> NotificationSourceKind;
    fn is_supporting_snoozed_notifications(&self) -> bool;
}

#[derive(Copy, Clone, PartialEq, Default, Debug, Display, EnumString, Serialize, Deserialize)]
pub enum NotificationListOrder {
    #[default]
    UpdatedAtAsc,
    UpdatedAtDesc,
}
