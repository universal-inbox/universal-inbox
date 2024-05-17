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
    third_party::integrations::todoist::TodoistItem,
    user::UserId,
    HasHtmlUrl,
};

use super::integrations::slack::SlackStar;

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
}

macro_attr! {
    #[derive(Copy, Clone, PartialEq, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum ThirdPartyItemKind {
        TodoistItem,
        SlackStar
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
        }
    }
}

impl ThirdPartyItemSource for ThirdPartyItem {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => ThirdPartyItemSourceKind::Todoist,
            ThirdPartyItemData::SlackStar(_) => ThirdPartyItemSourceKind::Slack,
        }
    }
}

impl ThirdPartyItem {
    pub fn kind(&self) -> ThirdPartyItemKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => ThirdPartyItemKind::TodoistItem,
            ThirdPartyItemData::SlackStar(_) => ThirdPartyItemKind::SlackStar,
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
        Todoist
    }
}

macro_attr! {
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum ThirdPartyItemSourceKind {
        Todoist,
        Slack
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
