use std::fmt;

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use url::Url;
use uuid::Uuid;

use crate::{
    notification::{Notification, NotificationMetadata, NotificationStatus},
    third_party::integrations::linear::{
        LinearIssue, LinearOrganization, LinearProject, LinearTeam, LinearUser,
    },
    user::UserId,
    HasHtmlUrl,
};

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(tag = "type", content = "content")]
#[allow(clippy::large_enum_variant)]
pub enum LinearNotification {
    IssueNotification {
        id: Uuid,
        r#type: String, // Could be an enum, but no exhaustive list of values (LinearWorkflowStateType?)
        read_at: Option<DateTime<Utc>>,
        updated_at: DateTime<Utc>,
        snoozed_until_at: Option<DateTime<Utc>>,
        organization: LinearOrganization,
        issue: LinearIssue,
        comment: Option<LinearComment>,
    },
    ProjectNotification {
        id: Uuid,
        r#type: String, // Could be an enum, but no exhaustive list of values
        read_at: Option<DateTime<Utc>>,
        updated_at: DateTime<Utc>,
        snoozed_until_at: Option<DateTime<Utc>>,
        organization: LinearOrganization,
        project: LinearProject,
        project_update: Option<LinearProjectUpdate>,
    },
}

impl LinearNotification {
    pub fn get_html_url_from_metadata(&self) -> Url {
        match self {
            LinearNotification::IssueNotification { issue, .. } => issue.get_html_url(),
            LinearNotification::ProjectNotification { project, .. } => project.get_html_url(),
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
                status: if read_at.is_some() {
                    NotificationStatus::Read
                } else {
                    NotificationStatus::Unread
                },
                metadata: NotificationMetadata::Linear(Box::new(self.clone())),
                updated_at: *updated_at,
                last_read_at: *read_at,
                snoozed_until: *snoozed_until_at,
                user_id,
                details: None,
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
                status: if read_at.is_some() {
                    NotificationStatus::Read
                } else {
                    NotificationStatus::Unread
                },
                metadata: NotificationMetadata::Linear(Box::new(self.clone())),
                updated_at: *updated_at,
                last_read_at: *read_at,
                snoozed_until: *snoozed_until_at,
                user_id,
                details: None,
                task_id: None,
            },
        }
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearProjectUpdate {
    pub updated_at: DateTime<Utc>,
    pub body: String,
    pub health: LinearProjectUpdateHealthType,
    pub user: LinearUser,
    pub url: Url,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum LinearProjectUpdateHealthType {
    OnTrack,
    AtRisk,
    OffTrack,
}

impl TryFrom<String> for LinearProjectUpdateHealthType {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "onTrack" => Ok(LinearProjectUpdateHealthType::OnTrack),
            "atRisk" => Ok(LinearProjectUpdateHealthType::AtRisk),
            "offTrack" => Ok(LinearProjectUpdateHealthType::OffTrack),
            _ => Err(anyhow!(
                "Unable to find LinearProjectUpdateHealthType value for `{}`",
                value
            )),
        }
    }
}

impl fmt::Display for LinearProjectUpdateHealthType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LinearProjectUpdateHealthType::OnTrack => "onTrack",
                LinearProjectUpdateHealthType::AtRisk => "atRisk",
                LinearProjectUpdateHealthType::OffTrack => "offTrack",
            }
        )
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearComment {
    pub updated_at: DateTime<Utc>,
    pub body: String,
    pub user: Option<LinearUser>,
    pub url: Url,
    pub children: Vec<LinearComment>,
}
