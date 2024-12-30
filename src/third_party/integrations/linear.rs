use std::fmt;

use anyhow::anyhow;
use chrono::{DateTime, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{serde_as, DisplayFromStr};
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::IntegrationConnectionId,
    task::{TaskPriority, TaskStatus},
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
    HasHtmlUrl,
};

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
    pub state_ids: Option<LinearWorkflowStateIds>, // Optional to be retro-compatible
}

impl LinearIssue {
    pub fn get_state_id_for_task_status(&self, status: TaskStatus) -> Option<Uuid> {
        match status {
            TaskStatus::Active => Some(self.state_ids.as_ref()?.unstarted),
            TaskStatus::Done => Some(self.state_ids.as_ref()?.completed),
            TaskStatus::Deleted => Some(self.state_ids.as_ref()?.canceled),
        }
    }
}

impl HasHtmlUrl for LinearIssue {
    fn get_html_url(&self) -> Url {
        self.url.clone()
    }
}

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
}

impl HasHtmlUrl for LinearNotification {
    fn get_html_url(&self) -> Url {
        match self {
            LinearNotification::IssueNotification { issue, .. } => issue.get_html_url(),
            LinearNotification::ProjectNotification { project, .. } => project.get_html_url(),
        }
    }
}

impl TryFrom<ThirdPartyItem> for LinearNotification {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::LinearNotification(linear_notification) => Ok(*linear_notification),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} into LinearNotification",
                item.id
            )),
        }
    }
}

impl ThirdPartyItemFromSource for LinearNotification {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        match &self {
            LinearNotification::IssueNotification { id, .. }
            | LinearNotification::ProjectNotification { id, .. } => ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: id.to_string(),
                data: ThirdPartyItemData::LinearNotification(Box::new(self.clone())),
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id,
                integration_connection_id,
                source_item: None,
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

impl From<LinearIssuePriority> for TaskPriority {
    fn from(priority: LinearIssuePriority) -> Self {
        match priority {
            LinearIssuePriority::NoPriority => TaskPriority::default(),
            LinearIssuePriority::Urgent => TaskPriority::P1,
            LinearIssuePriority::High => TaskPriority::P2,
            LinearIssuePriority::Normal => TaskPriority::P3,
            LinearIssuePriority::Low => TaskPriority::P4,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearTeam {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub icon: Option<String>,
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
    pub id: Option<Uuid>, // Optional to be retro-compatible
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq)]
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

impl From<LinearWorkflowStateType> for TaskStatus {
    fn from(state: LinearWorkflowStateType) -> Self {
        match state {
            LinearWorkflowStateType::Triage => TaskStatus::Active,
            LinearWorkflowStateType::Backlog => TaskStatus::Active,
            LinearWorkflowStateType::Unstarted => TaskStatus::Active,
            LinearWorkflowStateType::Started => TaskStatus::Active,
            LinearWorkflowStateType::Completed => TaskStatus::Done,
            LinearWorkflowStateType::Canceled => TaskStatus::Deleted,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearWorkflowStateIds {
    // Only gathering these ones for now as we won't use the others
    pub unstarted: Uuid,
    pub completed: Uuid,
    pub canceled: Uuid,
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

impl HasHtmlUrl for LinearProject {
    fn get_html_url(&self) -> Url {
        self.url.clone()
    }
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

impl LinearTeam {
    pub fn get_url(&self, organization: LinearOrganization) -> Url {
        format!("https://linear.app/{}/team/{}", organization.key, self.key)
            .parse::<Url>()
            .unwrap()
    }
}

impl TryFrom<ThirdPartyItem> for LinearIssue {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::LinearIssue(linear_issue) => Ok(*linear_issue),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} into LinearIssue",
                item.id
            )),
        }
    }
}

impl ThirdPartyItemFromSource for LinearIssue {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.id.to_string(),
            data: ThirdPartyItemData::LinearIssue(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }
}
