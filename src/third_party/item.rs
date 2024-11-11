use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use typed_id::TypedId;
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::{
        provider::{IntegrationProviderKind, IntegrationProviderSource},
        IntegrationConnectionId,
    },
    notification::Notification,
    task::Task,
    third_party::integrations::{
        linear::LinearIssue,
        slack::{SlackReaction, SlackStar},
        todoist::TodoistItem,
    },
    user::UserId,
    HasHtmlUrl,
};

use super::integrations::{
    github::GithubNotification,
    google_mail::GoogleMailThread,
    linear::{LinearNotification, LinearWorkflowState, LinearWorkflowStateType},
    slack::{SlackReactionState, SlackStarState},
};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThirdPartyItem {
    pub id: ThirdPartyItemId,
    pub source_id: String,
    pub data: ThirdPartyItemData,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_id: UserId,
    pub integration_connection_id: IntegrationConnectionId,
}

impl PartialEq for ThirdPartyItem {
    fn eq(&self, other: &Self) -> bool {
        self.source_id == other.source_id
            && self.data == other.data
            && self.user_id == other.user_id
            && self.integration_connection_id == other.integration_connection_id
    }
}

impl HasHtmlUrl for ThirdPartyItem {
    fn get_html_url(&self) -> Url {
        match self.data {
            ThirdPartyItemData::TodoistItem(ref item) => item.get_html_url(),
            ThirdPartyItemData::SlackStar(ref star) => star.get_html_url(),
            ThirdPartyItemData::SlackReaction(ref reaction) => reaction.get_html_url(),
            ThirdPartyItemData::LinearIssue(ref issue) => issue.get_html_url(),
            ThirdPartyItemData::LinearNotification(ref notification) => notification.get_html_url(),
            ThirdPartyItemData::GithubNotification(ref notification) => notification.get_html_url(),
            ThirdPartyItemData::GoogleMailThread(ref thread) => thread.get_html_url(),
        }
    }
}

pub type ThirdPartyItemId = TypedId<Uuid, ThirdPartyItem>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
pub enum ThirdPartyItemData {
    TodoistItem(Box<TodoistItem>),
    SlackStar(Box<SlackStar>),
    SlackReaction(Box<SlackReaction>),
    LinearIssue(Box<LinearIssue>),
    LinearNotification(Box<LinearNotification>),
    GithubNotification(Box<GithubNotification>),
    GoogleMailThread(Box<GoogleMailThread>),
}

macro_attr! {
    #[derive(Copy, Clone, PartialEq, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum ThirdPartyItemKind {
        TodoistItem,
        SlackStar,
        SlackReaction,
        LinearIssue,
        LinearNotification,
        GithubNotification,
        GoogleMailThread,
    }
}

pub trait ThirdPartyItemSource: IntegrationProviderSource {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind;
}

impl IntegrationProviderSource for ThirdPartyItem {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => IntegrationProviderKind::Todoist,
            ThirdPartyItemData::SlackStar(_) | ThirdPartyItemData::SlackReaction(_) => {
                IntegrationProviderKind::Slack
            }
            ThirdPartyItemData::LinearIssue(_) | ThirdPartyItemData::LinearNotification(_) => {
                IntegrationProviderKind::Linear
            }
            ThirdPartyItemData::GithubNotification(_) => IntegrationProviderKind::Github,
            ThirdPartyItemData::GoogleMailThread(_) => IntegrationProviderKind::GoogleMail,
        }
    }
}

impl ThirdPartyItemSource for ThirdPartyItem {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => ThirdPartyItemSourceKind::Todoist,
            ThirdPartyItemData::SlackStar(_) => ThirdPartyItemSourceKind::SlackStar,
            ThirdPartyItemData::SlackReaction(_) => ThirdPartyItemSourceKind::SlackReaction,
            ThirdPartyItemData::LinearIssue(_) => ThirdPartyItemSourceKind::LinearIssue,
            ThirdPartyItemData::LinearNotification(_) => {
                ThirdPartyItemSourceKind::LinearNotification
            }
            ThirdPartyItemData::GithubNotification(_) => {
                ThirdPartyItemSourceKind::GithubNotification
            }
            ThirdPartyItemData::GoogleMailThread(_) => ThirdPartyItemSourceKind::GoogleMailThread,
        }
    }
}

impl ThirdPartyItem {
    pub fn new(
        source_id: String,
        data: ThirdPartyItemData,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> Self {
        Self {
            id: Uuid::new_v4().into(),
            source_id,
            data,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
        }
    }

    pub fn kind(&self) -> ThirdPartyItemKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => ThirdPartyItemKind::TodoistItem,
            ThirdPartyItemData::SlackStar(_) => ThirdPartyItemKind::SlackStar,
            ThirdPartyItemData::SlackReaction(_) => ThirdPartyItemKind::SlackReaction,
            ThirdPartyItemData::LinearIssue(_) => ThirdPartyItemKind::LinearIssue,
            ThirdPartyItemData::LinearNotification(_) => ThirdPartyItemKind::LinearNotification,
            ThirdPartyItemData::GithubNotification(_) => ThirdPartyItemKind::GithubNotification,
            ThirdPartyItemData::GoogleMailThread(_) => ThirdPartyItemKind::GoogleMailThread,
        }
    }

    pub fn marked_as_done(&self) -> ThirdPartyItem {
        let new_data = match self.data {
            ThirdPartyItemData::TodoistItem(ref item) => {
                ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    checked: true,
                    completed_at: Some(Utc::now()),
                    ..*item.clone()
                }))
            }
            ThirdPartyItemData::SlackStar(ref slack_star) => {
                ThirdPartyItemData::SlackStar(Box::new(SlackStar {
                    state: SlackStarState::StarRemoved,
                    ..*slack_star.clone()
                }))
            }
            ThirdPartyItemData::SlackReaction(ref slack_reaction) => {
                ThirdPartyItemData::SlackReaction(Box::new(SlackReaction {
                    state: SlackReactionState::ReactionRemoved,
                    ..*slack_reaction.clone()
                }))
            }
            ThirdPartyItemData::LinearIssue(ref issue) => {
                ThirdPartyItemData::LinearIssue(Box::new(LinearIssue {
                    state: LinearWorkflowState {
                        r#type: LinearWorkflowStateType::Completed,
                        ..issue.state.clone()
                    },
                    completed_at: Some(Utc::now()),
                    ..*issue.clone()
                }))
            }
            _ => {
                return self.clone();
            }
        };

        ThirdPartyItem {
            data: new_data,
            updated_at: Utc::now(),
            ..self.clone()
        }
    }
}

pub trait ThirdPartyItemFromSource {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem;
}

macro_attr! {
    // Synchronization sources for third party items
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum ThirdPartyItemSyncSourceKind {
        Todoist,
        Linear,
        Github,
    }
}

macro_attr! {
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum ThirdPartyItemSourceKind {
        Todoist,
        SlackStar,
        SlackReaction,
        LinearIssue,
        LinearNotification,
        GithubNotification,
        GoogleMailThread,
    }
}

impl TryFrom<IntegrationProviderKind> for ThirdPartyItemSyncSourceKind {
    type Error = anyhow::Error;

    fn try_from(provider_kind: IntegrationProviderKind) -> Result<Self, Self::Error> {
        match provider_kind {
            IntegrationProviderKind::Todoist => Ok(Self::Todoist),
            IntegrationProviderKind::Linear => Ok(Self::Linear),
            IntegrationProviderKind::Github => Ok(Self::Github),
            _ => Err(anyhow!("IntegrationProviderKind {provider_kind} is not a valid ThirdPartyItemSyncSourceKind")),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ThirdPartyItemCreationResult {
    pub third_party_item: ThirdPartyItem,
    pub task: Option<Task>,
    pub notification: Option<Notification>,
}
