use anyhow::Context;
use uuid::Uuid;

use universal_inbox::{
    third_party::integrations::linear::{
        LinearIssue, LinearLabel, LinearProject, LinearProjectMilestone, LinearProjectStatus,
        LinearTeam, LinearUser, LinearWorkflowState, LinearWorkflowStateIds,
    },
    utils::emoji::replace_emoji_code_with_emoji,
};

use crate::universal_inbox::UniversalInboxError;

use super::assigned_issues_query;

impl TryFrom<assigned_issues_query::AssignedIssuesQueryIssuesNodesTeam> for LinearTeam {
    type Error = UniversalInboxError;

    fn try_from(
        value: assigned_issues_query::AssignedIssuesQueryIssuesNodesTeam,
    ) -> Result<Self, Self::Error> {
        Ok(LinearTeam {
            id: Uuid::parse_str(&value.id)
                .with_context(|| format!("Failed to parse UUID from `{}`", value.id))?,
            key: value.key,
            name: value.name,
            icon: value
                .icon
                .and_then(|icon| replace_emoji_code_with_emoji(&icon)),
        })
    }
}

impl TryFrom<assigned_issues_query::AssignedIssuesQueryIssuesNodesProjectLead> for LinearUser {
    type Error = UniversalInboxError;

    fn try_from(
        value: assigned_issues_query::AssignedIssuesQueryIssuesNodesProjectLead,
    ) -> Result<Self, Self::Error> {
        Ok(LinearUser {
            name: value.display_name,
            avatar_url: value
                .avatar_url
                .map(|avatar_url| {
                    avatar_url
                        .parse()
                        .with_context(|| format!("Failed to parse URL from `{avatar_url}`"))
                })
                .transpose()?,
            url: value
                .url
                .parse()
                .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
        })
    }
}

impl TryFrom<assigned_issues_query::AssignedIssuesQueryIssuesNodesProject> for LinearProject {
    type Error = UniversalInboxError;

    fn try_from(
        value: assigned_issues_query::AssignedIssuesQueryIssuesNodesProject,
    ) -> Result<Self, Self::Error> {
        Ok(LinearProject {
            id: Uuid::parse_str(&value.id)
                .with_context(|| format!("Failed to parse UUID from `{}`", value.id))?,
            name: value.name,
            url: value
                .url
                .parse()
                .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
            description: value.description,
            icon: value
                .icon
                .and_then(|icon| replace_emoji_code_with_emoji(&icon)),
            color: value.color,
            state: value.status.type_.into(),
            progress: (value.progress * 100.0).round() as i64,
            start_date: value.start_date,
            target_date: value.target_date,
            lead: value.lead.map(|lead| lead.try_into()).transpose()?,
        })
    }
}

impl From<assigned_issues_query::ProjectStatusType> for LinearProjectStatus {
    fn from(value: assigned_issues_query::ProjectStatusType) -> Self {
        match value {
            assigned_issues_query::ProjectStatusType::backlog => LinearProjectStatus::Backlog,
            assigned_issues_query::ProjectStatusType::planned => LinearProjectStatus::Planned,
            assigned_issues_query::ProjectStatusType::started => LinearProjectStatus::Started,
            assigned_issues_query::ProjectStatusType::paused => LinearProjectStatus::Paused,
            assigned_issues_query::ProjectStatusType::completed => LinearProjectStatus::Completed,
            assigned_issues_query::ProjectStatusType::canceled => LinearProjectStatus::Canceled,
            assigned_issues_query::ProjectStatusType::Other(_) => LinearProjectStatus::Backlog,
        }
    }
}

impl From<assigned_issues_query::AssignedIssuesQueryIssuesNodesProjectMilestone>
    for LinearProjectMilestone
{
    fn from(value: assigned_issues_query::AssignedIssuesQueryIssuesNodesProjectMilestone) -> Self {
        LinearProjectMilestone {
            name: value.name,
            description: value.description,
        }
    }
}

impl TryFrom<assigned_issues_query::AssignedIssuesQueryIssuesNodesCreator> for LinearUser {
    type Error = UniversalInboxError;

    fn try_from(
        value: assigned_issues_query::AssignedIssuesQueryIssuesNodesCreator,
    ) -> Result<Self, Self::Error> {
        Ok(LinearUser {
            name: value.display_name,
            avatar_url: value
                .avatar_url
                .map(|avatar_url| {
                    avatar_url
                        .parse()
                        .with_context(|| format!("Failed to parse URL from `{avatar_url}`"))
                })
                .transpose()?,
            url: value
                .url
                .parse()
                .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
        })
    }
}

impl TryFrom<assigned_issues_query::AssignedIssuesQueryIssuesNodesAssignee> for LinearUser {
    type Error = UniversalInboxError;

    fn try_from(
        value: assigned_issues_query::AssignedIssuesQueryIssuesNodesAssignee,
    ) -> Result<Self, Self::Error> {
        Ok(LinearUser {
            name: value.display_name,
            avatar_url: value
                .avatar_url
                .map(|avatar_url| {
                    avatar_url
                        .parse()
                        .with_context(|| format!("Failed to parse URL from `{avatar_url}`"))
                })
                .transpose()?,
            url: value
                .url
                .parse()
                .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
        })
    }
}

impl From<assigned_issues_query::AssignedIssuesQueryIssuesNodesLabelsNodes> for LinearLabel {
    fn from(value: assigned_issues_query::AssignedIssuesQueryIssuesNodesLabelsNodes) -> Self {
        LinearLabel {
            name: value.name,
            description: value.description,
            color: value.color,
        }
    }
}

impl TryFrom<assigned_issues_query::AssignedIssuesQueryIssuesNodesState> for LinearWorkflowState {
    type Error = UniversalInboxError;

    fn try_from(
        value: assigned_issues_query::AssignedIssuesQueryIssuesNodesState,
    ) -> Result<Self, Self::Error> {
        Ok(LinearWorkflowState {
            name: value.name,
            description: value.description,
            color: value.color,
            r#type: value.type_.try_into()?,
            id: Some(
                Uuid::parse_str(&value.id)
                    .with_context(|| format!("Failed to parse UUID from `{}`", value.id))?,
            ),
        })
    }
}

impl TryFrom<assigned_issues_query::AssignedIssuesQueryIssuesNodesStateTeamStates>
    for LinearWorkflowStateIds
{
    type Error = UniversalInboxError;

    fn try_from(
        value: assigned_issues_query::AssignedIssuesQueryIssuesNodesStateTeamStates,
    ) -> Result<Self, Self::Error> {
        let node = value
            .nodes
            .iter()
            .find(|node| node.type_ == "unstarted")
            .with_context(|| "Failed to find unstarted state id")?;
        let unstarted_id = Uuid::parse_str(&node.id)
            .with_context(|| format!("Failed to parse UUID from `{}`", node.id))?;
        let node = value
            .nodes
            .iter()
            .find(|node| node.type_ == "completed")
            .with_context(|| "Failed to find completed state id")?;
        let completed_id = Uuid::parse_str(&node.id)
            .with_context(|| format!("Failed to parse UUID from `{}`", node.id))?;
        let node = value
            .nodes
            .iter()
            .find(|node| node.type_ == "canceled")
            .with_context(|| "Failed to find canceled state id")?;
        let canceled_id = Uuid::parse_str(&node.id)
            .with_context(|| format!("Failed to parse UUID from `{}`", node.id))?;

        Ok(LinearWorkflowStateIds {
            unstarted: unstarted_id,
            completed: completed_id,
            canceled: canceled_id,
        })
    }
}

impl TryFrom<assigned_issues_query::AssignedIssuesQueryIssuesNodes> for LinearIssue {
    type Error = UniversalInboxError;

    fn try_from(
        value: assigned_issues_query::AssignedIssuesQueryIssuesNodes,
    ) -> Result<Self, Self::Error> {
        Ok(LinearIssue {
            id: Uuid::parse_str(&value.id)
                .with_context(|| format!("Failed to parse UUID from `{}`", value.id))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            completed_at: value.completed_at,
            canceled_at: value.canceled_at,
            due_date: value.due_date,
            identifier: value.identifier,
            title: value.title,
            url: value
                .url
                .parse()
                .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
            priority: value.priority.try_into()?,
            project: value
                .project
                .map(|project| project.try_into())
                .transpose()?,
            project_milestone: value
                .project_milestone
                .map(|project_milestone| project_milestone.into()),
            creator: value
                .creator
                .map(|creator| creator.try_into())
                .transpose()?,
            assignee: value
                .assignee
                .map(|assignee| assignee.try_into())
                .transpose()?,
            state: value.state.clone().try_into()?,
            labels: value
                .labels
                .nodes
                .into_iter()
                .map(|label| label.into())
                .collect(),
            description: value.description,
            team: value.team.try_into()?,
            state_ids: Some(value.state.team.states.try_into()?),
        })
    }
}

impl TryFrom<assigned_issues_query::ResponseData> for Vec<LinearIssue> {
    type Error = UniversalInboxError;

    fn try_from(value: assigned_issues_query::ResponseData) -> Result<Self, Self::Error> {
        value
            .issues
            .nodes
            .into_iter()
            .map(|issue| issue.try_into())
            .collect()
    }
}
