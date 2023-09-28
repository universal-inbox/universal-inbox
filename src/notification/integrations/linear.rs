use chrono::{DateTime, Utc};
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use crate::{
    notification::{Notification, NotificationMetadata, NotificationStatus},
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
        snoozed_until_at: Option<DateTime<Utc>>,
        organization: LinearOrganization,
        issue: LinearIssue,
    },
    ProjectNotification {
        id: Uuid,
        r#type: String, // Could be an enum, but no exhaustive list of values
        read_at: Option<DateTime<Utc>>,
        updated_at: DateTime<Utc>,
        snoozed_until_at: Option<DateTime<Utc>>,
        organization: LinearOrganization,
        project: LinearProject,
    },
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearIssue {
    pub id: Uuid,
    pub identifier: String,
    pub title: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    pub team: LinearTeam,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearTeam {
    pub id: Uuid,
    pub key: String,
    pub name: String,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearProject {
    pub id: Uuid,
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearOrganization {
    pub name: String,
    pub key: String,
}

impl LinearNotification {
    pub fn get_html_url_from_metadata(&self) -> Uri {
        match self {
            LinearNotification::IssueNotification {
                issue: LinearIssue { url, .. },
                ..
            } => url.clone(),
            LinearNotification::ProjectNotification {
                project: LinearProject { url, .. },
                ..
            } => url.clone(),
        }
    }

    pub fn get_type(&self) -> String {
        match self {
            LinearNotification::IssueNotification { r#type, .. }
            | LinearNotification::ProjectNotification { r#type, .. } => r#type.clone(),
        }
    }

    pub fn get_organization(&self) -> LinearOrganization {
        match self {
            LinearNotification::IssueNotification { organization, .. }
            | LinearNotification::ProjectNotification { organization, .. } => organization.clone(),
        }
    }

    pub fn get_team(&self) -> Option<LinearTeam> {
        match self {
            LinearNotification::IssueNotification {
                issue: LinearIssue { team, .. },
                ..
            } => Some(team.clone()),
            LinearNotification::ProjectNotification { .. } => None,
        }
    }

    pub fn into_notification(self, user_id: UserId) -> Notification {
        match &self {
            LinearNotification::IssueNotification {
                id,
                read_at,
                updated_at,
                snoozed_until_at,
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
                snoozed_until: *snoozed_until_at,
                user_id,
                task_id: None,
            },
            LinearNotification::ProjectNotification {
                id,
                read_at,
                updated_at,
                snoozed_until_at,
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
                snoozed_until: *snoozed_until_at,
                user_id,
                task_id: None,
            },
        }
    }
}

impl LinearTeam {
    pub fn get_url(&self, organization: LinearOrganization) -> Uri {
        format!("https://linear.app/{}/team/{}", organization.key, self.key)
            .parse::<Uri>()
            .unwrap()
    }
}
