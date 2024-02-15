use std::fmt;

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use slack_morphism::prelude::SlackPushEventCallback;
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::integrations::{
        github::{GithubNotification, GithubPullRequest},
        google_mail::GoogleMailThread,
        linear::LinearNotification,
    },
    task::{integrations::todoist::DEFAULT_TODOIST_HTML_URL, Task, TaskId},
    user::UserId,
    HasHtmlUrl,
};

use self::integrations::github::GithubDiscussion;

pub mod integrations;
pub mod service;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Notification {
    pub id: NotificationId,
    pub title: String,
    pub source_id: String,
    pub status: NotificationStatus,
    pub metadata: NotificationMetadata,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub details: Option<NotificationDetails>,
}

impl HasHtmlUrl for Notification {
    // tag: New notification integration
    fn get_html_url(&self) -> Url {
        if let Some(details) = &self.details {
            details.get_html_url()
        } else {
            match &self.metadata {
                NotificationMetadata::Github(github_notification) => {
                    github_notification.get_html_url_from_metadata()
                }
                NotificationMetadata::Todoist => DEFAULT_TODOIST_HTML_URL.parse::<Url>().unwrap(),
                NotificationMetadata::Linear(linear_notification) => {
                    linear_notification.get_html_url_from_metadata()
                }
                NotificationMetadata::GoogleMail(google_mail_thread) => {
                    google_mail_thread.get_html_url_from_metadata()
                }
                NotificationMetadata::Slack(_) => {
                    // TODO: it requires to call Slack API to get the message URL
                    // See https://api.slack.com/methods/chat.getPermalink
                    // Hardcoding it for now
                    "https://slack.com".parse::<Url>().unwrap()
                }
            }
        }
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct NotificationWithTask {
    pub id: NotificationId,
    pub title: String,
    pub source_id: String,
    pub status: NotificationStatus,
    pub metadata: NotificationMetadata,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub user_id: UserId,
    pub task: Option<Task>,
    pub details: Option<NotificationDetails>,
}

impl HasHtmlUrl for NotificationWithTask {
    fn get_html_url(&self) -> Url {
        match &self {
            NotificationWithTask {
                metadata: NotificationMetadata::Todoist,
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
            source_id: notification.source_id.clone(),
            status: notification.status,
            metadata: notification.metadata.clone(),
            updated_at: notification.updated_at,
            last_read_at: notification.last_read_at,
            snoozed_until: notification.snoozed_until,
            user_id: notification.user_id,
            details: notification.details.clone(),
            task,
        }
    }

    pub fn is_built_from_task(&self) -> bool {
        matches!(self.metadata, NotificationMetadata::Todoist)
    }

    pub fn into_notification(self) -> Notification {
        Notification {
            id: self.id,
            title: self.title.clone(),
            source_id: self.source_id.clone(),
            status: self.status,
            metadata: self.metadata.clone(),
            updated_at: self.updated_at,
            last_read_at: self.last_read_at,
            snoozed_until: self.snoozed_until,
            user_id: self.user_id,
            details: self.details,
            task_id: self.task.as_ref().map(|task| task.id),
        }
    }
}

// tag: New notification integration
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
pub enum NotificationMetadata {
    Github(Box<GithubNotification>),
    Todoist,
    Linear(Box<LinearNotification>),
    GoogleMail(Box<GoogleMailThread>),
    Slack(Box<SlackPushEventCallback>),
}

// tag: New notification integration
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum NotificationDetails {
    GithubPullRequest(GithubPullRequest),
    GithubDiscussion(GithubDiscussion),
}

impl HasHtmlUrl for NotificationDetails {
    // tag: New notification integration
    fn get_html_url(&self) -> Url {
        match &self {
            NotificationDetails::GithubPullRequest(GithubPullRequest { url, .. }) => url.clone(),
            NotificationDetails::GithubDiscussion(GithubDiscussion { url, .. }) => url.clone(),
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
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationSourceKind {
        Github,
        Todoist,
        Linear,
        GoogleMail,
        Slack
    }
}

impl TryFrom<IntegrationProviderKind> for NotificationSyncSourceKind {
    type Error = ();

    // tag: New notification integration
    fn try_from(provider_kind: IntegrationProviderKind) -> Result<Self, Self::Error> {
        match provider_kind {
            IntegrationProviderKind::Github => Ok(Self::Github),
            IntegrationProviderKind::Linear => Ok(Self::Linear),
            IntegrationProviderKind::GoogleMail => Ok(Self::GoogleMail),
            _ => Err(()),
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
            IntegrationProviderKind::Todoist => Ok(Self::Todoist),
            IntegrationProviderKind::Slack => Ok(Self::Slack),
            _ => Err(()),
        }
    }
}

pub trait NotificationSource: IntegrationProviderSource {
    fn get_notification_source_kind(&self) -> NotificationSourceKind;
    fn is_supporting_snoozed_notifications(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use chrono::{TimeZone, Utc};

    use super::*;
    use rstest::*;

    mod get_html_url {
        use pretty_assertions::assert_eq;

        use crate::task::{
            integrations::todoist::TodoistItem, TaskMetadata, TaskPriority, TaskStatus,
        };

        use super::*;

        #[fixture]
        pub fn github_notification() -> Box<GithubNotification> {
            let fixture_path = format!(
                "{}/tests/fixtures/github_notification.json",
                env::var("CARGO_MANIFEST_DIR").unwrap(),
            );
            let input_str = fs::read_to_string(fixture_path).unwrap();
            serde_json::from_str(&input_str).unwrap()
        }

        #[fixture]
        pub fn todoist_item() -> Box<TodoistItem> {
            let fixture_path = format!(
                "{}/tests/fixtures/todoist_item.json",
                env::var("CARGO_MANIFEST_DIR").unwrap(),
            );
            let input_str = fs::read_to_string(fixture_path).unwrap();
            serde_json::from_str(&input_str).unwrap()
        }

        #[rstest]
        fn test_get_html_url_for_github_pr_notification(
            github_notification: Box<GithubNotification>,
        ) {
            let expected_url: Url = "https://github.com/octokit/octokit.rb/issues/123"
                .parse()
                .unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                metadata: NotificationMetadata::Github(github_notification),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                details: None,
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }

        #[rstest]
        fn test_get_html_url_for_github_ci_notification(
            mut github_notification: Box<GithubNotification>,
        ) {
            github_notification.subject.r#type = "CheckSuite".to_string();
            let expected_url: Url = "https://github.com/octocat/Hello-World/actions"
                .parse()
                .unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                metadata: NotificationMetadata::Github(github_notification),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                details: None,
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }

        #[rstest]
        fn test_get_html_url_for_github_discussion_notification(
            mut github_notification: Box<GithubNotification>,
        ) {
            github_notification.subject.r#type = "Discussion".to_string();
            github_notification.subject.title = "Test with spaces".to_string();
            let expected_url: Url =
                "https://github.com/octocat/Hello-World/discussions?discussions_q=Test+with+spaces"
                    .parse()
                    .unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                metadata: NotificationMetadata::Github(github_notification),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                details: None,
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }

        #[rstest]
        fn test_get_html_url_for_todoist_notification(todoist_item: Box<TodoistItem>) {
            let expected_url: Url = "https://todoist.com/showTask?id=456".parse().unwrap();
            let notification = NotificationWithTask {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                metadata: NotificationMetadata::Todoist,
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                details: None,
                task: Some(Task {
                    id: Uuid::new_v4().into(),
                    source_id: "456".to_string(),
                    title: "task1".to_string(),
                    body: "test".to_string(),
                    status: TaskStatus::Done,
                    completed_at: None,
                    priority: TaskPriority::P1,
                    due_at: None,
                    tags: vec![],
                    parent_id: None,
                    project: "Project".to_string(),
                    is_recurring: false,
                    created_at: Utc::now(),
                    metadata: TaskMetadata::Todoist(*todoist_item),
                    user_id: Uuid::new_v4().into(),
                }),
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }

        #[rstest]
        fn test_get_html_url_for_todoist_notification_without_task() {
            let expected_url: Url = "https://todoist.com/app/".parse().unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                metadata: NotificationMetadata::Todoist,
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                details: None,
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }
    }
}
