use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use slack_blocks_render::render_blocks_as_markdown;
use slack_morphism::{
    SlackChannelId, SlackChannelInfo, SlackFileCommentId, SlackFileId, SlackHistoryMessage,
    SlackMessageOrigin, SlackTs,
};
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::IntegrationConnectionId,
    notification::integrations::slack::{
        SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
        SlackImDetails, SlackMessageDetails,
    },
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
    utils::emoji::replace_emoji_code_in_string_with_emoji,
    HasHtmlUrl,
};

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackStar {
    pub state: SlackStarState,
    pub created_at: DateTime<Utc>,
    pub starred_item: SlackStarredItem,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
pub enum SlackStarState {
    StarAdded,
    StarRemoved,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
pub enum SlackStarredItem {
    SlackMessage(SlackMessageDetails),
    SlackFile(SlackFileDetails),
    SlackFileComment(SlackFileCommentDetails),
    SlackChannel(SlackChannelDetails),
    SlackIm(SlackImDetails),
    SlackGroup(SlackGroupDetails),
}

impl SlackStarredItem {
    pub fn id(&self) -> String {
        match self {
            SlackStarredItem::SlackMessage(message) => message.message.origin.ts.to_string(),
            SlackStarredItem::SlackFile(file) => file.id.as_ref().unwrap().to_string(), // Can use unwrap because new SlackStar all have an `id` value
            SlackStarredItem::SlackFileComment(comment) => comment.comment_id.to_string(),
            SlackStarredItem::SlackChannel(channel) => channel.channel.id.to_string(),
            SlackStarredItem::SlackIm(im) => im.channel.id.to_string(),
            SlackStarredItem::SlackGroup(group) => group.channel.id.to_string(),
        }
    }

    pub fn content(&self) -> String {
        match self {
            SlackStarredItem::SlackMessage(message) => {
                if let Some(blocks) = &message.message.content.blocks {
                    if !blocks.is_empty() {
                        return render_blocks_as_markdown(blocks.clone());
                    }
                }

                if let Some(attachments) = &message.message.content.attachments {
                    if !attachments.is_empty() {
                        let str_blocks = attachments
                            .iter()
                            .filter_map(|a| {
                                a.blocks
                                    .as_ref()
                                    .map(|blocks| render_blocks_as_markdown(blocks.clone()))
                            })
                            .collect::<Vec<String>>();
                        if !str_blocks.is_empty() {
                            return str_blocks.join("\n");
                        }
                    }
                }

                replace_emoji_code_in_string_with_emoji(
                    &message
                        .message
                        .content
                        .text
                        .clone()
                        .unwrap_or("Starred message".to_string()),
                )
            }
            SlackStarredItem::SlackFile(file) => file
                .title
                .clone()
                .unwrap_or_else(|| "Starred file".to_string()),
            SlackStarredItem::SlackFileComment(comment) => comment.comment_id.to_string(),
            SlackStarredItem::SlackChannel(channel) => channel
                .channel
                .name
                .clone()
                .unwrap_or_else(|| "Starred channel".to_string()),
            SlackStarredItem::SlackIm(im) => im
                .channel
                .name
                .clone()
                .unwrap_or_else(|| "Starred IM".to_string()),
            SlackStarredItem::SlackGroup(group) => group
                .channel
                .name
                .clone()
                .unwrap_or_else(|| "Starred group".to_string()),
        }
    }

    pub fn ids(&self) -> SlackStarIds {
        let (channel_id, message_id, file_id, file_comment_id) = match &self {
            SlackStarredItem::SlackMessage(SlackMessageDetails {
                channel: SlackChannelInfo { id: channel_id, .. },
                message:
                    SlackHistoryMessage {
                        origin: SlackMessageOrigin { ts, .. },
                        ..
                    },
                ..
            }) => (Some(channel_id.clone()), Some(ts.clone()), None, None),
            SlackStarredItem::SlackChannel(SlackChannelDetails {
                channel: SlackChannelInfo { id, .. },
                ..
            }) => (Some(id.clone()), None, None, None),
            SlackStarredItem::SlackIm(SlackImDetails {
                channel: SlackChannelInfo { id, .. },
                ..
            }) => (Some(id.clone()), None, None, None),
            SlackStarredItem::SlackGroup(SlackGroupDetails {
                channel: SlackChannelInfo { id, .. },
                ..
            }) => (Some(id.clone()), None, None, None),
            SlackStarredItem::SlackFile(SlackFileDetails {
                id: file_id,
                channel: SlackChannelInfo { id: channel_id, .. },
                ..
            }) => (Some(channel_id.clone()), None, file_id.clone(), None),
            SlackStarredItem::SlackFileComment(SlackFileCommentDetails {
                comment_id,
                channel: SlackChannelInfo { id: channel_id, .. },
                ..
            }) => (
                Some(channel_id.clone()),
                None,
                None,
                Some(comment_id.clone()),
            ),
        };

        SlackStarIds {
            channel_id,
            message_id,
            file_id,
            file_comment_id,
        }
    }
}

impl HasHtmlUrl for SlackStar {
    fn get_html_url(&self) -> Url {
        match &self.starred_item {
            SlackStarredItem::SlackMessage(message) => message.get_html_url(),
            SlackStarredItem::SlackFile(file) => file.get_html_url(),
            SlackStarredItem::SlackFileComment(comment) => comment.get_html_url(),
            SlackStarredItem::SlackChannel(channel) => channel.get_html_url(),
            SlackStarredItem::SlackIm(im) => im.get_html_url(),
            SlackStarredItem::SlackGroup(group) => group.get_html_url(),
        }
    }
}

impl TryFrom<ThirdPartyItem> for SlackStar {
    type Error = ();

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::SlackStar(slack_star) => Ok(slack_star),
            _ => Err(()),
        }
    }
}

impl ThirdPartyItemFromSource for SlackStar {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        let source_id = self.starred_item.id();

        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id,
            data: ThirdPartyItemData::SlackStar(self.clone()),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
        }
    }
}

pub struct SlackStarIds {
    pub channel_id: Option<SlackChannelId>,
    pub message_id: Option<SlackTs>,
    pub file_id: Option<SlackFileId>,
    pub file_comment_id: Option<SlackFileCommentId>,
}

#[cfg(test)]
mod test {
    use std::{env, fs};

    use crate::notification::integrations::slack::SlackMessageSenderDetails;

    use super::*;
    use rstest::*;
    use slack_morphism::{
        api::{
            SlackApiConversationsHistoryResponse, SlackApiConversationsInfoResponse,
            SlackApiTeamInfoResponse, SlackApiUsersInfoResponse,
        },
        SlackMessageAttachment,
    };

    pub fn fixture_path(fixture_file_name: &str) -> String {
        format!(
            "{}/tests/fixtures/{fixture_file_name}",
            env::var("CARGO_MANIFEST_DIR").unwrap()
        )
    }
    pub fn load_json_fixture_file<T: for<'de> serde::de::Deserialize<'de>>(
        fixture_file_name: &str,
    ) -> T {
        let input_str = fs::read_to_string(fixture_path(fixture_file_name)).unwrap();
        serde_json::from_str::<T>(&input_str).unwrap()
    }

    #[fixture]
    pub fn slack_starred_message() -> Box<SlackStarredItem> {
        let message_response: SlackApiConversationsHistoryResponse =
            load_json_fixture_file("slack_fetch_message_response.json");
        let channel_response: SlackApiConversationsInfoResponse =
            load_json_fixture_file("slack_fetch_channel_response.json");
        let user_response: SlackApiUsersInfoResponse =
            load_json_fixture_file("slack_fetch_user_response.json");
        let sender = SlackMessageSenderDetails::User(Box::new(user_response.user));
        let team_response: SlackApiTeamInfoResponse =
            load_json_fixture_file("slack_fetch_team_response.json");

        Box::new(SlackStarredItem::SlackMessage(SlackMessageDetails {
            url: "https://example.com".parse().unwrap(),
            message: message_response.messages[0].clone(),
            channel: channel_response.channel,
            sender,
            team: team_response.team,
        }))
    }

    #[rstest]
    fn test_render_starred_message_with_blocks(slack_starred_message: Box<SlackStarredItem>) {
        assert_eq!(
            slack_starred_message.content(),
            "🔴  *Test title* 🔴\n\n- list 1\n- list 2\n1. number 1\n1. number 2\n> quote\n```$ echo Hello world```\n_Some_ `formatted` ~text~.\n\nHere is a [link](https://www.universal-inbox.com)"
        );
    }

    #[rstest]
    fn test_render_starred_message_with_blocks_in_attachments(
        mut slack_starred_message: Box<SlackStarredItem>,
    ) {
        let SlackStarredItem::SlackMessage(message) = &mut (*slack_starred_message) else {
            panic!(
                "Expected SlackStarredItem::SlackMessage, got {:?}",
                slack_starred_message
            );
        };
        message.message.content.attachments = Some(vec![SlackMessageAttachment {
            id: None,
            color: None,
            fallback: None,
            title: None,
            fields: None,
            mrkdwn_in: None,
            text: None,
            blocks: message.message.content.blocks.clone(),
        }]);
        message.message.content.blocks = Some(vec![]);
        assert_eq!(
            slack_starred_message.content(),
            "🔴  *Test title* 🔴\n\n- list 1\n- list 2\n1. number 1\n1. number 2\n> quote\n```$ echo Hello world```\n_Some_ `formatted` ~text~.\n\nHere is a [link](https://www.universal-inbox.com)"
        );
    }

    #[rstest]
    fn test_render_starred_message_with_only_text(
        mut slack_starred_message: Box<SlackStarredItem>,
    ) {
        let SlackStarredItem::SlackMessage(message) = &mut (*slack_starred_message) else {
            panic!(
                "Expected SlackStarredItem::SlackMessage, got {:?}",
                slack_starred_message
            );
        };
        message.message.content.text = Some("Test message".to_string());
        message.message.content.blocks = Some(vec![]);
        assert_eq!(slack_starred_message.content(), "Test message".to_string());
    }
}
