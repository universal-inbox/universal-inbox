use chrono::{DateTime, Utc};
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
    third_party::integrations::{linear::LinearIssue, slack::SlackStar, todoist::TodoistItem},
    user::UserId,
    HasHtmlUrl,
};

use super::integrations::{
    linear::{LinearWorkflowState, LinearWorkflowStateType},
    slack::SlackStarState,
};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ThirdPartyItem {
    pub id: ThirdPartyItemId,
    pub source_id: String,
    pub data: ThirdPartyItemData,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_id: UserId,
    pub integration_connection_id: IntegrationConnectionId,
}

impl HasHtmlUrl for ThirdPartyItem {
    fn get_html_url(&self) -> Url {
        match self.data {
            ThirdPartyItemData::TodoistItem(ref item) => item.get_html_url(),
            ThirdPartyItemData::SlackStar(ref star) => star.get_html_url(),
            ThirdPartyItemData::LinearIssue(ref issue) => issue.get_html_url(),
        }
    }
}

pub type ThirdPartyItemId = TypedId<Uuid, ThirdPartyItem>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
#[allow(clippy::large_enum_variant)]
pub enum ThirdPartyItemData {
    TodoistItem(TodoistItem),
    SlackStar(SlackStar),
    LinearIssue(LinearIssue),
}

macro_attr! {
    #[derive(Copy, Clone, PartialEq, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum ThirdPartyItemKind {
        TodoistItem,
        SlackStar,
        LinearIssue
    }
}

pub trait ThirdPartyItemSource: IntegrationProviderSource {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind;
}

impl IntegrationProviderSource for ThirdPartyItem {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => IntegrationProviderKind::Todoist,
            ThirdPartyItemData::SlackStar(_) => IntegrationProviderKind::Slack,
            ThirdPartyItemData::LinearIssue(_) => IntegrationProviderKind::Linear,
        }
    }
}

impl ThirdPartyItemSource for ThirdPartyItem {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => ThirdPartyItemSourceKind::Todoist,
            ThirdPartyItemData::SlackStar(_) => ThirdPartyItemSourceKind::Slack,
            ThirdPartyItemData::LinearIssue(_) => ThirdPartyItemSourceKind::Linear,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            user_id,
            integration_connection_id,
        }
    }

    pub fn kind(&self) -> ThirdPartyItemKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => ThirdPartyItemKind::TodoistItem,
            ThirdPartyItemData::SlackStar(_) => ThirdPartyItemKind::SlackStar,
            ThirdPartyItemData::LinearIssue(_) => ThirdPartyItemKind::LinearIssue,
        }
    }

    pub fn marked_as_done(&self) -> ThirdPartyItem {
        let new_data = match self.data {
            ThirdPartyItemData::TodoistItem(ref item) => {
                ThirdPartyItemData::TodoistItem(TodoistItem {
                    checked: true,
                    completed_at: Some(Utc::now()),
                    ..item.clone()
                })
            }
            ThirdPartyItemData::SlackStar(ref slack_star) => {
                ThirdPartyItemData::SlackStar(SlackStar {
                    state: SlackStarState::StarRemoved,
                    ..slack_star.clone()
                })
            }
            ThirdPartyItemData::LinearIssue(ref issue) => {
                ThirdPartyItemData::LinearIssue(LinearIssue {
                    state: LinearWorkflowState {
                        r#type: LinearWorkflowStateType::Completed,
                        ..issue.state.clone()
                    },
                    completed_at: Some(Utc::now()),
                    ..issue.clone()
                })
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
        Linear
    }
}

macro_attr! {
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum ThirdPartyItemSourceKind {
        Todoist,
        Slack,
        Linear
    }
}

impl TryFrom<IntegrationProviderKind> for ThirdPartyItemSyncSourceKind {
    type Error = ();

    fn try_from(provider_kind: IntegrationProviderKind) -> Result<Self, Self::Error> {
        match provider_kind {
            IntegrationProviderKind::Todoist => Ok(Self::Todoist),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ThirdPartyItemCreationResult {
    pub third_party_item: ThirdPartyItem,
    pub task: Option<Task>,
    pub notification: Option<Notification>,
}
