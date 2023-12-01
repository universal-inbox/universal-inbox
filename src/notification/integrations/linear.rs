use std::fmt;

use anyhow::anyhow;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{serde_as, DisplayFromStr};
use url::Url;
use uuid::Uuid;

use crate::{
    notification::{Notification, NotificationMetadata, NotificationStatus},
    user::UserId,
};

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(tag = "type", content = "content")]
#[allow(clippy::large_enum_variant)] // TODO - review later
pub enum LinearNotification {
    IssueNotification {
        id: Uuid,
        r#type: String, // Could be an enum, but no exhaustive list of values (LinearWorkflowStateType?)
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub due_date: Option<NaiveDate>,
    pub identifier: String,
    pub title: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Url,
    pub priority: LinearIssuePriority,
    pub project: Option<LinearProject>,
    pub project_milestone: Option<LinearProjectMilestone>,
    pub creator: Option<LinearUser>,
    pub assignee: Option<LinearUser>,
    pub state: LinearWorkflowState,
    pub labels: Vec<LinearLabel>,
    pub description: Option<String>,
    pub team: LinearTeam,
}

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Debug, Clone, Eq, Copy)]
#[repr(u8)]
pub enum LinearIssuePriority {
    NoPriority = 0,
    Urgent = 1,
    High = 2,
    Normal = 3,
    Low = 4,
}

impl fmt::Display for LinearIssuePriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LinearIssuePriority::NoPriority => "",
                LinearIssuePriority::Urgent => "Urgent",
                LinearIssuePriority::High => "High",
                LinearIssuePriority::Normal => "Normal",
                LinearIssuePriority::Low => "Low",
            }
        )
    }
}

impl TryFrom<f64> for LinearIssuePriority {
    type Error = anyhow::Error;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        match value as u8 {
            0 => Ok(LinearIssuePriority::NoPriority),
            1 => Ok(LinearIssuePriority::Urgent),
            2 => Ok(LinearIssuePriority::High),
            3 => Ok(LinearIssuePriority::Normal),
            4 => Ok(LinearIssuePriority::Low),
            _ => Err(anyhow!(
                "Unable to find LinearIssuePriority value for `{}`",
                value
            )),
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearTeam {
    pub id: Uuid,
    pub key: String,
    pub name: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearProjectMilestone {
    pub name: String,
    pub description: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearUser {
    pub name: String,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub avatar_url: Option<Url>,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Url,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearLabel {
    pub name: String,
    pub description: Option<String>,
    pub color: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearWorkflowState {
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub r#type: LinearWorkflowStateType,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum LinearWorkflowStateType {
    Triage,
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
}

impl TryFrom<String> for LinearWorkflowStateType {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "triage" => Ok(LinearWorkflowStateType::Triage),
            "backlog" => Ok(LinearWorkflowStateType::Backlog),
            "unstarted" => Ok(LinearWorkflowStateType::Unstarted),
            "started" => Ok(LinearWorkflowStateType::Started),
            "completed" => Ok(LinearWorkflowStateType::Completed),
            "canceled" => Ok(LinearWorkflowStateType::Canceled),
            _ => Err(anyhow!(
                "Unable to find LinearWorkflowStateType value for `{}`",
                value
            )),
        }
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearProject {
    pub id: Uuid,
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Url,
    pub description: String,
    pub icon: Option<String>,
    pub color: String,
    pub state: LinearProjectState,
    pub progress: i64, // percentage between 0 and 100
    pub start_date: Option<NaiveDate>,
    pub target_date: Option<NaiveDate>,
    pub lead: Option<LinearUser>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum LinearProjectState {
    Planned,
    Backlog,
    Started,
    Paused,
    Completed,
    Canceled,
}

impl TryFrom<String> for LinearProjectState {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "planned" => Ok(LinearProjectState::Planned),
            "backlog" => Ok(LinearProjectState::Backlog),
            "started" => Ok(LinearProjectState::Started),
            "paused" => Ok(LinearProjectState::Paused),
            "completed" => Ok(LinearProjectState::Completed),
            "canceled" => Ok(LinearProjectState::Canceled),
            _ => Err(anyhow!(
                "Unable to find LinearProjectState value for `{}`",
                value
            )),
        }
    }
}

impl fmt::Display for LinearProjectState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LinearProjectState::Backlog => "backlog",
                LinearProjectState::Canceled => "canceled",
                LinearProjectState::Completed => "completed",
                LinearProjectState::Paused => "paused",
                LinearProjectState::Planned => "planned",
                LinearProjectState::Started => "started",
            }
        )
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearOrganization {
    pub name: String,
    pub key: String,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub logo_url: Option<Url>,
}

impl LinearNotification {
    pub fn get_html_url_from_metadata(&self) -> Url {
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
                source_html_url: Some(project.url.clone()),
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

impl LinearTeam {
    pub fn get_url(&self, organization: LinearOrganization) -> Url {
        format!("https://linear.app/{}/team/{}", organization.key, self.key)
            .parse::<Url>()
            .unwrap()
    }
}
