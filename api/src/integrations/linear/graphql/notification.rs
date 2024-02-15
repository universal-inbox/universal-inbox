use anyhow::Context;
use uuid::Uuid;

use universal_inbox::{
    notification::integrations::linear::{
        LinearComment, LinearIssue, LinearLabel, LinearNotification, LinearOrganization,
        LinearProject, LinearProjectMilestone, LinearProjectUpdate, LinearProjectUpdateHealthType,
        LinearTeam, LinearUser, LinearWorkflowState,
    },
    utils::emoji::replace_emoji_code_with_emoji,
};

use crate::{
    integrations::linear::graphql::notifications_query, universal_inbox::UniversalInboxError,
};

impl TryFrom<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueTeam>
    for LinearTeam
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueTeam,
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

impl
    TryFrom<
            notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueProjectLead,
        > for LinearUser
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueProjectLead,
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

impl
    TryFrom<
        notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueProject,
    > for LinearProject
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueProject,
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
            state: value.state.try_into()?,
            progress: (value.progress * 100.0).round() as i64,
            start_date: value.start_date,
            target_date: value.target_date,
            lead: value.lead.map(|lead| lead.try_into()).transpose()?,
        })
    }
}

impl From<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueProjectMilestone> for LinearProjectMilestone {
    fn from(value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueProjectMilestone) -> Self {
        LinearProjectMilestone {
            name: value.name,
            description: value.description,
        }
    }
}

impl
    TryFrom<
        notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueCreator,
    > for LinearUser
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueCreator,
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

impl
    TryFrom<
        notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueAssignee,
    > for LinearUser
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueAssignee,
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

impl
    TryFrom<
        notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProjectLead,
    > for LinearUser
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProjectLead,
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

impl From<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueLabelsNodes> for LinearLabel {
    fn from(value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueLabelsNodes) -> Self {
        LinearLabel {
            name: value.name,
            description: value.description,
            color: value.color,
        }
    }
}

impl TryFrom<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueState>
    for LinearWorkflowState
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssueState,
    ) -> Result<Self, Self::Error> {
        Ok(LinearWorkflowState {
            name: value.name,
            description: value.description,
            color: value.color,
            r#type: value.type_.try_into()?,
        })
    }
}

impl TryFrom<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssue>
    for LinearIssue
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssue,
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
            state: value.state.try_into()?,
            labels: value
                .labels
                .nodes
                .into_iter()
                .map(|label| label.into())
                .collect(),
            description: value.description,
            team: value.team.try_into()?,
        })
    }
}

impl TryFrom<notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProject>
    for LinearProject
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProject,
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
            state: value.state.try_into()?,
            progress: (value.progress * 100.0).round() as i64,
            start_date: value.start_date,
            target_date: value.target_date,
            lead: value.lead.map(|lead| lead.try_into()).transpose()?,
        })
    }
}

impl From<notifications_query::ProjectUpdateHealthType> for LinearProjectUpdateHealthType {
    fn from(value: notifications_query::ProjectUpdateHealthType) -> Self {
        match value {
            notifications_query::ProjectUpdateHealthType::onTrack => {
                LinearProjectUpdateHealthType::OnTrack
            }
            notifications_query::ProjectUpdateHealthType::atRisk => {
                LinearProjectUpdateHealthType::AtRisk
            }
            notifications_query::ProjectUpdateHealthType::offTrack => {
                LinearProjectUpdateHealthType::OffTrack
            }
            notifications_query::ProjectUpdateHealthType::Other(_) => {
                LinearProjectUpdateHealthType::OnTrack
            }
        }
    }
}

impl
    TryFrom<
            notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProjectUpdateUser,
        > for LinearUser
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProjectUpdateUser,
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

impl
    TryFrom<
        notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProjectUpdate,
    > for LinearProjectUpdate
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProjectUpdate,
    ) -> Result<Self, Self::Error> {
        Ok(LinearProjectUpdate {
            updated_at: value.updated_at,
            body: value.body,
            health: value.health.into(),
            user: value.user.try_into()?,
            url: value
                .url
                .parse()
                .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
        })
    }
}

impl TryFrom<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationComment>
    for LinearComment
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationComment,
    ) -> Result<Self, Self::Error> {
        if let Some(parent) = value.parent {
            parent.try_into()
        } else {
            Ok(LinearComment {
                updated_at: value.updated_at,
                body: value.body,
                url: value
                    .url
                    .parse()
                    .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
                user: value.user.map(|user| user.try_into()).transpose()?,
                children: vec![],
            })
        }
    }
}

impl
    TryFrom<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentUser>
    for LinearUser
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentUser,
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

impl
    TryFrom<
        notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentParent,
    > for LinearComment
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentParent,
    ) -> Result<Self, Self::Error> {
        Ok(LinearComment {
            updated_at: value.updated_at,
            body: value.body,
            url: value
                .url
                .parse()
                .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
            user: value.user.map(|user| user.try_into()).transpose()?,
            children: value
                .children
                .nodes
                .into_iter()
                .map(|comment| comment.try_into())
                .collect::<Result<Vec<LinearComment>, UniversalInboxError>>()?,
        })
    }
}

impl
    TryFrom<
            notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentParentUser,
        > for LinearUser
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentParentUser,
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

impl TryFrom<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentParentChildrenNodes>
    for LinearComment
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentParentChildrenNodes,
    ) -> Result<Self, Self::Error> {
        Ok(LinearComment {
            updated_at: value.updated_at,
            body: value.body,
            url: value
                .url
                .parse()
                .with_context(|| format!("Failed to parse URL from `{}`", value.url))?,
            user: value.user.map(|user| user.try_into()).transpose()?,
            children: vec![]
        })
    }
}

impl
    TryFrom<notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentParentChildrenNodesUser>
    for LinearUser
{
    type Error = UniversalInboxError;

    fn try_from(
        value: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationCommentParentChildrenNodesUser,
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

impl TryFrom<notifications_query::ResponseData> for Vec<LinearNotification> {
    type Error = UniversalInboxError;

    fn try_from(value: notifications_query::ResponseData) -> Result<Self, Self::Error> {
        let organization_name = value.organization.name.clone();
        let organization_key = value.organization.url_key.clone();
        let organization_logo_url = value
            .organization
            .logo_url
            .map(|logo_url| {
                logo_url
                    .parse()
                    .with_context(|| format!("Failed to parse URL from `{logo_url}`"))
            })
            .transpose()?;

        value
            .notifications
            .nodes
            .into_iter()
            .map(|notification| match notification {
                notifications_query::NotificationsQueryNotificationsNodes {
                    id,
                    type_,
                    read_at,
                    updated_at,
                    snoozed_until_at,
                    on: notifications_query::NotificationsQueryNotificationsNodesOn::IssueNotification(notifications_query::NotificationsQueryNotificationsNodesOnIssueNotification {
                        issue,
                        comment,
                    }),
                } => Ok(Some(LinearNotification::IssueNotification {
                    id: Uuid::parse_str(&id).with_context(|| format!("Failed to parse UUID from `{id}`"))?,
                    r#type: type_,
                    read_at,
                    updated_at,
                    snoozed_until_at,
                    organization: LinearOrganization {
                        name: organization_name.clone(),
                        key: organization_key.clone(),
                        logo_url: organization_logo_url.clone(),
                    },
                    issue: issue.try_into()?,
                    comment: comment.map(|comment| comment.try_into()).transpose()?,
                })),
                notifications_query::NotificationsQueryNotificationsNodes {
                    id,
                    type_,
                    read_at,
                    updated_at,
                    snoozed_until_at,
                    on: notifications_query::NotificationsQueryNotificationsNodesOn::ProjectNotification(notifications_query::NotificationsQueryNotificationsNodesOnProjectNotification {
                        project,
                        project_update
                    }),
                } => Ok(Some(LinearNotification::ProjectNotification {
                    id: Uuid::parse_str(&id).with_context(|| format!("Failed to parse UUID from `{id}`"))?,
                    r#type: type_,
                    read_at,
                    updated_at,
                    snoozed_until_at,
                    organization: LinearOrganization {
                        name: organization_name.clone(),
                        key: organization_key.clone(),
                        logo_url: organization_logo_url.clone(),
                    },
                    project: project.try_into()?,
                    project_update: project_update.map(|update| update.try_into()).transpose()?,
                })),
                // Ignoring any other type of notifications
                _ => Ok(None)
            })
            .filter_map(|linear_notification_result| linear_notification_result.transpose())
            .collect()
    }
}
