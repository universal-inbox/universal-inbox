use chrono::{DateTime, Utc};
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use crate::{
    notification::{IntoNotification, Notification, NotificationMetadata, NotificationStatus},
    user::UserId,
};

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(tag = "__typename")]
pub enum LinearNotification {
    IssueNotification {
        id: Uuid,
        r#type: String, // Could be an enum, but no exhaustive list of values
        read_at: Option<DateTime<Utc>>,
        updated_at: DateTime<Utc>,
        issue: LinearIssue,
    },
    ProjectNotification {
        id: Uuid,
        r#type: String, // Could be an enum, but no exhaustive list of values
        read_at: Option<DateTime<Utc>>,
        updated_at: DateTime<Utc>,
        project: LinearProject,
    },
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearIssue {
    pub id: Uuid,
    pub title: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearProject {
    pub id: Uuid,
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
}

impl IntoNotification for LinearNotification {
    fn into_notification(self, user_id: UserId) -> Notification {
        match &self {
            LinearNotification::IssueNotification {
                id,
                read_at,
                updated_at,
                issue,
                ..
            } => Notification {
                id: Uuid::new_v4().into(),
                title: issue.title.clone(),
                source_id: id.to_string(),
                source_html_url: Some(issue.url.clone()),
                status: if read_at.is_some() {
                    NotificationStatus::Read
                } else {
                    NotificationStatus::Unread
                },
                metadata: NotificationMetadata::Linear(self.clone()),
                updated_at: *updated_at,
                last_read_at: *read_at,
                snoozed_until: None,
                user_id,
                task_id: None,
            },
            LinearNotification::ProjectNotification {
                id,
                read_at,
                updated_at,
                project,
                ..
            } => Notification {
                id: Uuid::new_v4().into(),
                title: project.name.clone(),
                source_id: id.to_string(),
                source_html_url: Some(project.url.clone()),
                status: if read_at.is_some() {
                    NotificationStatus::Read
                } else {
                    NotificationStatus::Unread
                },
                metadata: NotificationMetadata::Linear(self.clone()),
                updated_at: *updated_at,
                last_read_at: *read_at,
                snoozed_until: None,
                user_id,
                task_id: None,
            },
        }
    }
}
