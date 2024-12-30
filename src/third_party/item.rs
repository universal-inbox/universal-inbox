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
        github::GithubNotification,
        google_calendar::GoogleCalendarEvent,
        google_mail::GoogleMailThread,
        linear::{LinearIssue, LinearNotification, LinearWorkflowState, LinearWorkflowStateType},
        slack::{SlackReaction, SlackReactionState, SlackStar, SlackStarState, SlackThread},
        todoist::TodoistItem,
    },
    user::UserId,
    HasHtmlUrl,
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
    pub source_item: Option<Box<ThirdPartyItem>>,
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
            ThirdPartyItemData::SlackThread(ref thread) => thread.get_html_url(),
            ThirdPartyItemData::LinearIssue(ref issue) => issue.get_html_url(),
            ThirdPartyItemData::LinearNotification(ref notification) => notification.get_html_url(),
            ThirdPartyItemData::GithubNotification(ref notification) => notification.get_html_url(),
            ThirdPartyItemData::GoogleMailThread(ref thread) => thread.get_html_url(),
            ThirdPartyItemData::GoogleCalendarEvent(ref event) => event.get_html_url(),
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
    SlackThread(Box<SlackThread>),
    LinearIssue(Box<LinearIssue>),
    LinearNotification(Box<LinearNotification>),
    GithubNotification(Box<GithubNotification>),
    GoogleMailThread(Box<GoogleMailThread>),
    GoogleCalendarEvent(Box<GoogleCalendarEvent>),
}

macro_attr! {
    #[derive(Copy, Clone, PartialEq, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum ThirdPartyItemKind {
        TodoistItem,
        SlackStar,
        SlackReaction,
        SlackThread,
        LinearIssue,
        LinearNotification,
        GithubNotification,
        GoogleMailThread,
        GoogleCalendarEvent,
    }
}

pub trait ThirdPartyItemSource: IntegrationProviderSource {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind;
}

impl IntegrationProviderSource for ThirdPartyItem {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => IntegrationProviderKind::Todoist,
            ThirdPartyItemData::SlackStar(_)
            | ThirdPartyItemData::SlackReaction(_)
            | ThirdPartyItemData::SlackThread(_) => IntegrationProviderKind::Slack,
            ThirdPartyItemData::LinearIssue(_) | ThirdPartyItemData::LinearNotification(_) => {
                IntegrationProviderKind::Linear
            }
            ThirdPartyItemData::GithubNotification(_) => IntegrationProviderKind::Github,
            ThirdPartyItemData::GoogleMailThread(_) => IntegrationProviderKind::GoogleMail,
            ThirdPartyItemData::GoogleCalendarEvent(_) => IntegrationProviderKind::GoogleCalendar,
        }
    }
}

impl ThirdPartyItemSource for ThirdPartyItem {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => ThirdPartyItemSourceKind::Todoist,
            ThirdPartyItemData::SlackStar(_) => ThirdPartyItemSourceKind::SlackStar,
            ThirdPartyItemData::SlackReaction(_) => ThirdPartyItemSourceKind::SlackReaction,
            ThirdPartyItemData::SlackThread(_) => ThirdPartyItemSourceKind::SlackThread,
            ThirdPartyItemData::LinearIssue(_) => ThirdPartyItemSourceKind::LinearIssue,
            ThirdPartyItemData::LinearNotification(_) => {
                ThirdPartyItemSourceKind::LinearNotification
            }
            ThirdPartyItemData::GithubNotification(_) => {
                ThirdPartyItemSourceKind::GithubNotification
            }
            ThirdPartyItemData::GoogleMailThread(_) => ThirdPartyItemSourceKind::GoogleMailThread,
            ThirdPartyItemData::GoogleCalendarEvent(_) => {
                ThirdPartyItemSourceKind::GoogleCalendarEvent
            }
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
            source_item: None,
        }
    }

    pub fn kind(&self) -> ThirdPartyItemKind {
        match self.data {
            ThirdPartyItemData::TodoistItem(_) => ThirdPartyItemKind::TodoistItem,
            ThirdPartyItemData::SlackStar(_) => ThirdPartyItemKind::SlackStar,
            ThirdPartyItemData::SlackReaction(_) => ThirdPartyItemKind::SlackReaction,
            ThirdPartyItemData::SlackThread(_) => ThirdPartyItemKind::SlackThread,
            ThirdPartyItemData::LinearIssue(_) => ThirdPartyItemKind::LinearIssue,
            ThirdPartyItemData::LinearNotification(_) => ThirdPartyItemKind::LinearNotification,
            ThirdPartyItemData::GithubNotification(_) => ThirdPartyItemKind::GithubNotification,
            ThirdPartyItemData::GoogleMailThread(_) => ThirdPartyItemKind::GoogleMailThread,
            ThirdPartyItemData::GoogleCalendarEvent(_) => ThirdPartyItemKind::GoogleCalendarEvent,
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
            ThirdPartyItemData::SlackThread(ref slack_thread) => {
                let mut messages = slack_thread.messages.clone();
                let last_message = slack_thread.messages.last();
                let first_message = messages.first_mut();
                first_message.parent.last_read = Some(last_message.origin.ts.clone());
                ThirdPartyItemData::SlackThread(Box::new(SlackThread {
                    messages,
                    ..*slack_thread.clone()
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
        SlackThread,
        LinearIssue,
        LinearNotification,
        GithubNotification,
        GoogleMailThread,
        GoogleCalendarEvent,
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::*;
    use slack_morphism::api::{
        SlackApiConversationsHistoryResponse, SlackApiConversationsInfoResponse,
        SlackApiTeamInfoResponse,
    };

    use crate::test_helpers::load_json_fixture_file;

    use super::*;

    #[fixture]
    pub fn slack_thread_third_party_item() -> ThirdPartyItem {
        let message_response: SlackApiConversationsHistoryResponse =
            load_json_fixture_file("slack_fetch_thread_response.json");
        let channel_response: SlackApiConversationsInfoResponse =
            load_json_fixture_file("slack_fetch_channel_response.json");
        let team_response: SlackApiTeamInfoResponse =
            load_json_fixture_file("slack_fetch_team_response.json");

        let slack_thread = SlackThread {
            url: "https://example.com".parse().unwrap(),
            messages: message_response.messages.try_into().unwrap(),
            subscribed: true,
            last_read: None,
            channel: channel_response.channel.clone(),
            team: team_response.team.clone(),
            references: None,
            sender_profiles: Default::default(),
        };

        ThirdPartyItem::new(
            "123".to_string(),
            ThirdPartyItemData::SlackThread(Box::new(slack_thread)),
            Uuid::new_v4().into(),
            Uuid::new_v4().into(),
        )
    }

    #[rstest]
    fn test_mark_as_done_a_slack_thread(slack_thread_third_party_item: ThirdPartyItem) {
        let ThirdPartyItemData::SlackThread(ref slack_thread) = slack_thread_third_party_item.data
        else {
            unreachable!("Expected SlackThread data");
        };
        let first_message = slack_thread.messages.first();
        assert_eq!(
            first_message.parent.last_read,
            Some(first_message.origin.ts.clone())
        );

        let ThirdPartyItemData::SlackThread(ref slack_thread_marked_as_done) =
            slack_thread_third_party_item.marked_as_done().data
        else {
            unreachable!("Expected SlackThread data");
        };
        let first_message = slack_thread_marked_as_done.messages.first();
        let last_message = slack_thread_marked_as_done.messages.last();
        assert_eq!(
            first_message.parent.last_read,
            Some(last_message.origin.ts.clone())
        );
    }
}
