use std::fmt;

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use crate::{
    integration_connection::{IntegrationProvider, IntegrationProviderKind},
    notification::integrations::{github::GithubNotification, linear::LinearNotification},
    task::{integrations::todoist::DEFAULT_TODOIST_HTML_URL, Task, TaskId},
    user::UserId,
    HasHtmlUrl,
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

impl HasHtmlUrl for Notification {
    fn get_html_url(&self) -> Uri {
        self.source_html_url
            .clone()
            .unwrap_or_else(|| match &self.metadata {
                NotificationMetadata::Github(github_notification) => {
                    github_notification.get_html_url_from_metadata()
                }
                NotificationMetadata::Todoist => DEFAULT_TODOIST_HTML_URL.parse::<Uri>().unwrap(),
                NotificationMetadata::Linear(linear_notification) => {
                    linear_notification.get_html_url_from_metadata()
                }
            })
    }
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

impl HasHtmlUrl for NotificationWithTask {
    fn get_html_url(&self) -> Uri {
        Notification::from(self.clone()).get_html_url()
    }
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

    pub fn is_built_from_task(&self) -> bool {
        matches!(self.metadata, NotificationMetadata::Todoist)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum NotificationMetadata {
    Github(GithubNotification),
    Todoist,
    Linear(LinearNotification),
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
    // Synchronization sources for notifications
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationSyncSourceKind {
        Github,
        Linear
    }
}

macro_attr! {
    // notification sources, either direct or from tasks
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationSourceKind {
        Github,
        Todoist,
        Linear
    }
}

impl TryFrom<IntegrationProviderKind> for NotificationSyncSourceKind {
    type Error = ();

    fn try_from(provider_kind: IntegrationProviderKind) -> Result<Self, Self::Error> {
        match provider_kind {
            IntegrationProviderKind::Github => Ok(Self::Github),
            _ => Err(()),
        }
    }
}

pub trait NotificationSource: IntegrationProvider {
    fn get_notification_source_kind(&self) -> NotificationSourceKind;
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use chrono::{TimeZone, Utc};

    use super::*;
    use rstest::*;

    mod get_html_url {
        use pretty_assertions::assert_eq;

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

        #[rstest]
        fn test_get_html_url_for_github_pr_notification(
            github_notification: Box<GithubNotification>,
        ) {
            let expected_url: Uri = "https://github.com/octokit/octokit.rb/issues/123"
                .parse()
                .unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                source_html_url: Some(expected_url.clone()),
                metadata: NotificationMetadata::Github(*github_notification),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }

        #[rstest]
        fn test_get_html_url_for_github_ci_notification(
            mut github_notification: Box<GithubNotification>,
        ) {
            github_notification.subject.r#type = "CheckSuite".to_string();
            let expected_url: Uri = "https://github.com/octocat/Hello-World/actions"
                .parse()
                .unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                source_html_url: None,
                metadata: NotificationMetadata::Github(*github_notification),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
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
            let expected_url: Uri =
                "https://github.com/octocat/Hello-World/discussions?discussions_q=Test+with+spaces"
                    .parse()
                    .unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                source_html_url: None,
                metadata: NotificationMetadata::Github(*github_notification),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }

        #[rstest]
        fn test_get_html_url_for_github_notification_with_no_source_html_url(
            github_notification: Box<GithubNotification>,
        ) {
            let expected_url: Uri = "https://github.com/octocat/Hello-World".parse().unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                source_html_url: None,
                metadata: NotificationMetadata::Github(*github_notification),
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }

        #[rstest]
        fn test_get_html_url_for_todoist_notification() {
            let expected_url: Uri = "https://todoist.com/app/project/123/task/456"
                .parse()
                .unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                source_html_url: Some(expected_url.clone()),
                metadata: NotificationMetadata::Todoist,
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }

        #[rstest]
        fn test_get_html_url_for_todoist_notification_with_no_source_html_url() {
            let expected_url: Uri = "https://todoist.com/app/".parse().unwrap();
            let notification = Notification {
                id: Uuid::new_v4().into(),
                title: "notif1".to_string(),
                status: NotificationStatus::Unread,
                source_id: "1234".to_string(),
                source_html_url: None,
                metadata: NotificationMetadata::Todoist,
                updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                last_read_at: None,
                snoozed_until: None,
                user_id: Uuid::new_v4().into(),
                task_id: None,
            };

            assert_eq!(notification.get_html_url(), expected_url);
        }
    }
}
