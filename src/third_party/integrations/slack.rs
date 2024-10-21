use chrono::{DateTime, Timelike, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use slack_blocks_render::{
    render_blocks_as_markdown, text::render_blocks_as_text, SlackReferences,
};
use slack_morphism::prelude::*;
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::IntegrationConnectionId,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
    utils::{emoji::replace_emoji_code_in_string_with_emoji, truncate::truncate_with_ellipse},
    HasHtmlUrl,
};

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackStar {
    pub state: SlackStarState,
    pub created_at: DateTime<Utc>,
    pub item: SlackStarItem,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
pub enum SlackStarState {
    StarAdded,
    StarRemoved,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
pub enum SlackStarItem {
    SlackMessage(SlackMessageDetails),
    SlackFile(SlackFileDetails),
    SlackFileComment(SlackFileCommentDetails),
    SlackChannel(SlackChannelDetails),
    SlackIm(SlackImDetails),
    SlackGroup(SlackGroupDetails),
}

impl SlackStarItem {
    pub fn id(&self) -> String {
        match self {
            SlackStarItem::SlackMessage(message) => message.message.origin.ts.to_string(),
            SlackStarItem::SlackFile(file) => file.id.as_ref().unwrap().to_string(), // Can use unwrap because new SlackStar all have an `id` value
            SlackStarItem::SlackFileComment(comment) => comment.comment_id.to_string(),
            SlackStarItem::SlackChannel(channel) => channel.channel.id.to_string(),
            SlackStarItem::SlackIm(im) => im.channel.id.to_string(),
            SlackStarItem::SlackGroup(group) => group.channel.id.to_string(),
        }
    }

    pub fn title(&self) -> String {
        match self {
            SlackStarItem::SlackMessage(message) => message.title(),
            SlackStarItem::SlackFile(file) => file.title(),
            SlackStarItem::SlackFileComment(comment) => comment.comment_id.to_string(),
            SlackStarItem::SlackChannel(channel) => channel
                .channel
                .name
                .clone()
                .unwrap_or_else(|| "Channel".to_string()),
            SlackStarItem::SlackIm(im) => {
                im.channel.name.clone().unwrap_or_else(|| "IM".to_string())
            }
            SlackStarItem::SlackGroup(group) => group
                .channel
                .name
                .clone()
                .unwrap_or_else(|| "Group".to_string()),
        }
    }

    pub fn content(&self) -> String {
        match self {
            SlackStarItem::SlackMessage(message) => message.content(),
            SlackStarItem::SlackFile(file) => file.content(),
            SlackStarItem::SlackFileComment(comment) => comment.comment_id.to_string(),
            SlackStarItem::SlackChannel(channel) => channel
                .channel
                .name
                .clone()
                .unwrap_or_else(|| "Channel".to_string()),
            SlackStarItem::SlackIm(im) => {
                im.channel.name.clone().unwrap_or_else(|| "IM".to_string())
            }
            SlackStarItem::SlackGroup(group) => group
                .channel
                .name
                .clone()
                .unwrap_or_else(|| "Group".to_string()),
        }
    }

    pub fn ids(&self) -> SlackStarIds {
        let (channel_id, message_id, file_id, file_comment_id) = match &self {
            SlackStarItem::SlackMessage(SlackMessageDetails {
                channel: SlackChannelInfo { id: channel_id, .. },
                message:
                    SlackHistoryMessage {
                        origin: SlackMessageOrigin { ts, .. },
                        ..
                    },
                ..
            }) => (Some(channel_id.clone()), Some(ts.clone()), None, None),
            SlackStarItem::SlackChannel(SlackChannelDetails {
                channel: SlackChannelInfo { id, .. },
                ..
            }) => (Some(id.clone()), None, None, None),
            SlackStarItem::SlackIm(SlackImDetails {
                channel: SlackChannelInfo { id, .. },
                ..
            }) => (Some(id.clone()), None, None, None),
            SlackStarItem::SlackGroup(SlackGroupDetails {
                channel: SlackChannelInfo { id, .. },
                ..
            }) => (Some(id.clone()), None, None, None),
            SlackStarItem::SlackFile(SlackFileDetails {
                id: file_id,
                channel: SlackChannelInfo { id: channel_id, .. },
                ..
            }) => (Some(channel_id.clone()), None, file_id.clone(), None),
            SlackStarItem::SlackFileComment(SlackFileCommentDetails {
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
        match &self.item {
            SlackStarItem::SlackMessage(message) => message.get_html_url(),
            SlackStarItem::SlackFile(file) => file.get_html_url(),
            SlackStarItem::SlackFileComment(comment) => comment.get_html_url(),
            SlackStarItem::SlackChannel(channel) => channel.get_html_url(),
            SlackStarItem::SlackIm(im) => im.get_html_url(),
            SlackStarItem::SlackGroup(group) => group.get_html_url(),
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
        let source_id = self.item.id();

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

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackReaction {
    pub name: SlackReactionName,
    pub state: SlackReactionState,
    pub created_at: DateTime<Utc>,
    pub item: SlackReactionItem,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
pub enum SlackReactionState {
    ReactionAdded,
    ReactionRemoved,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "content")]
#[allow(clippy::large_enum_variant)]
pub enum SlackReactionItem {
    SlackMessage(SlackMessageDetails),
    SlackFile(SlackFileDetails),
}

impl SlackReactionItem {
    pub fn id(&self) -> String {
        match self {
            SlackReactionItem::SlackMessage(message) => message.message.origin.ts.to_string(),
            SlackReactionItem::SlackFile(file) => file.id.as_ref().unwrap().to_string(), // Can use unwrap because new SlackReaction all have an `id` value
        }
    }

    pub fn title(&self) -> String {
        match self {
            SlackReactionItem::SlackMessage(message) => message.title(),
            SlackReactionItem::SlackFile(file) => file.title(),
        }
    }

    pub fn content(&self) -> String {
        match self {
            SlackReactionItem::SlackMessage(message) => message.content(),
            SlackReactionItem::SlackFile(file) => file.content(),
        }
    }

    pub fn ids(&self) -> Option<SlackReactionIds> {
        let (channel_id, message_id) = match &self {
            SlackReactionItem::SlackMessage(SlackMessageDetails {
                channel: SlackChannelInfo { id: channel_id, .. },
                message:
                    SlackHistoryMessage {
                        origin: SlackMessageOrigin { ts, .. },
                        ..
                    },
                ..
            }) => (channel_id.clone(), ts.clone()),
            _ => return None,
        };

        Some(SlackReactionIds {
            channel_id,
            message_id,
        })
    }
}

impl HasHtmlUrl for SlackReaction {
    fn get_html_url(&self) -> Url {
        match &self.item {
            SlackReactionItem::SlackMessage(message) => message.get_html_url(),
            SlackReactionItem::SlackFile(file) => file.get_html_url(),
        }
    }
}

impl TryFrom<ThirdPartyItem> for SlackReaction {
    type Error = ();

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::SlackReaction(slack_reaction) => Ok(slack_reaction),
            _ => Err(()),
        }
    }
}

impl ThirdPartyItemFromSource for SlackReaction {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        let source_id = self.item.id();

        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id,
            data: ThirdPartyItemData::SlackReaction(self.clone()),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
        }
    }
}

pub struct SlackReactionIds {
    pub channel_id: SlackChannelId,
    pub message_id: SlackTs,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackMessageDetails {
    pub url: Url,
    pub message: SlackHistoryMessage,
    pub channel: SlackChannelInfo,
    pub sender: SlackMessageSenderDetails,
    pub team: SlackTeamInfo,
    pub references: Option<SlackReferences>,
}

impl HasHtmlUrl for SlackMessageDetails {
    fn get_html_url(&self) -> Url {
        self.url.clone()
    }
}

impl SlackMessageDetails {
    pub fn get_channel_html_url(&self) -> Url {
        format!(
            "https://app.slack.com/client/{}/{}",
            self.team.id, self.channel.id
        )
        .parse()
        .unwrap()
    }

    pub fn title(&self) -> String {
        if let Some(attachments) = &self.message.content.attachments {
            if let Some(first_attachment) = attachments.first() {
                if let Some(title) = first_attachment.title.as_ref() {
                    return title.clone();
                }
            }
        }

        truncate_with_ellipse(&self.content_(false), 50, "...", true)
    }

    pub fn content(&self) -> String {
        self.content_(true)
    }

    fn content_(&self, as_markdown: bool) -> String {
        if let Some(blocks) = &self.message.content.blocks {
            if !blocks.is_empty() {
                return if as_markdown {
                    render_blocks_as_markdown(
                        blocks.clone(),
                        self.references.clone().unwrap_or_default(),
                    )
                } else {
                    render_blocks_as_text(
                        blocks.clone(),
                        self.references.clone().unwrap_or_default(),
                    )
                };
            }
        }

        if let Some(attachments) = &self.message.content.attachments {
            if !attachments.is_empty() {
                let str_blocks = attachments
                    .iter()
                    .filter_map(|a| {
                        if let Some(blocks) = a.blocks.as_ref() {
                            return if as_markdown {
                                Some(render_blocks_as_markdown(
                                    blocks.clone(),
                                    self.references.clone().unwrap_or_default(),
                                ))
                            } else {
                                Some(render_blocks_as_text(
                                    blocks.clone(),
                                    self.references.clone().unwrap_or_default(),
                                ))
                            };
                        }

                        if let Some(text) = a.text.as_ref() {
                            let sanitized_text = sanitize_slack_markdown(text);
                            if let Some(title) = a.title.as_ref() {
                                return Some(format!("{}\n\n{}", title, sanitized_text));
                            }

                            return Some(sanitized_text);
                        }

                        None
                    })
                    .collect::<Vec<String>>();

                if !str_blocks.is_empty() {
                    return str_blocks.join("\n");
                }
            }
        }

        let message = if let Some(text) = &self.message.content.text {
            sanitize_slack_markdown(text)
        } else {
            "A slack message".to_string()
        };

        replace_emoji_code_in_string_with_emoji(&message)
    }
}

fn sanitize_slack_markdown(slack_markdown: &str) -> String {
    // Replace slack markdown with common markdown
    // This could be more robustly implemented using Slack blocks
    let regexs = [
        (Regex::new(r"^```").unwrap(), "```\n"),
        (Regex::new(r"```$").unwrap(), "\n```"),
        (Regex::new(r"^• ").unwrap(), "- "),
        (Regex::new(r"^(\s*)◦ ").unwrap(), "$1- "),
        (Regex::new(r"^&gt; ").unwrap(), "> "),
        (Regex::new(r"<([^|]+)\|([^>]+)>").unwrap(), "[$2]($1)"),
    ];

    slack_markdown
        .lines()
        .map(|line| {
            regexs
                .iter()
                .fold(line.to_string(), |acc, (re, replacement)| {
                    re.replace(&acc, *replacement).to_string()
                })
        })
        .collect::<Vec<String>>()
        .join("\n")
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
#[serde(tag = "type", content = "content")]
pub enum SlackMessageSenderDetails {
    User(Box<SlackUser>),
    Bot(SlackBotInfo),
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackFileDetails {
    pub id: Option<SlackFileId>, // Option to ease the transition when the field is added
    pub title: Option<String>,
    pub channel: SlackChannelInfo,
    pub sender: Option<SlackUser>,
    pub team: SlackTeamInfo,
}

impl HasHtmlUrl for SlackFileDetails {
    fn get_html_url(&self) -> Url {
        format!(
            "https://app.slack.com/client/{}/{}",
            self.team.id, self.channel.id
        )
        .parse()
        .unwrap()
    }
}

impl SlackFileDetails {
    pub fn title(&self) -> String {
        self.title.clone().unwrap_or_else(|| "File".to_string())
    }

    pub fn content(&self) -> String {
        self.title.clone().unwrap_or_else(|| "File".to_string())
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackFileCommentDetails {
    pub channel: SlackChannelInfo,
    pub comment_id: SlackFileCommentId,
    pub sender: Option<SlackUser>,
    pub team: SlackTeamInfo,
}

impl HasHtmlUrl for SlackFileCommentDetails {
    fn get_html_url(&self) -> Url {
        format!(
            "https://app.slack.com/client/{}/{}",
            self.team.id, self.channel.id
        )
        .parse()
        .unwrap()
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackChannelDetails {
    pub channel: SlackChannelInfo,
    pub team: SlackTeamInfo,
}

impl HasHtmlUrl for SlackChannelDetails {
    fn get_html_url(&self) -> Url {
        format!(
            "https://app.slack.com/client/{}/{}",
            self.team.id, self.channel.id
        )
        .parse()
        .unwrap()
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackImDetails {
    pub channel: SlackChannelInfo,
    pub team: SlackTeamInfo,
}

impl HasHtmlUrl for SlackImDetails {
    fn get_html_url(&self) -> Url {
        format!(
            "https://app.slack.com/client/{}/{}",
            self.team.id, self.channel.id
        )
        .parse()
        .unwrap()
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackGroupDetails {
    pub channel: SlackChannelInfo,
    pub team: SlackTeamInfo,
}

impl HasHtmlUrl for SlackGroupDetails {
    fn get_html_url(&self) -> Url {
        format!(
            "https://app.slack.com/client/{}/{}",
            self.team.id, self.channel.id
        )
        .parse()
        .unwrap()
    }
}

#[cfg(test)]
mod test_sanitize_slack_markdown {
    use super::*;
    use rstest::*;

    #[rstest]
    fn test_sanitize_slack_markdown_code() {
        assert_eq!(
            sanitize_slack_markdown("```$ echo Hello```"),
            "```\n$ echo Hello\n```"
        );
        assert_eq!(
            sanitize_slack_markdown("test: ```$ echo Hello```."),
            "test: ```$ echo Hello```."
        );
    }

    #[rstest]
    fn test_sanitize_slack_markdown_list() {
        assert_eq!(sanitize_slack_markdown("• item"), "- item");
        assert_eq!(sanitize_slack_markdown("test: • item"), "test: • item");
    }

    #[rstest]
    fn test_sanitize_slack_markdown_sublist() {
        assert_eq!(sanitize_slack_markdown(" ◦ subitem"), " - subitem");
        assert_eq!(
            sanitize_slack_markdown("test: ◦ subitem"),
            "test: ◦ subitem"
        );
    }

    #[rstest]
    fn test_sanitize_slack_markdown_quote() {
        assert_eq!(sanitize_slack_markdown("&gt; "), "> ");
        assert_eq!(sanitize_slack_markdown("test: &gt; "), "test: &gt; ");
    }

    #[rstest]
    fn test_sanitize_slack_markdown_link() {
        assert_eq!(
            sanitize_slack_markdown("This is a <https://www.example.com|link> to www.example.com"),
            "This is a [link](https://www.example.com) to www.example.com"
        );
    }
}

#[cfg(test)]
mod test_message_details {
    use std::{env, fs};

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
    pub fn slack_starred_message() -> Box<SlackStarItem> {
        let message_response: SlackApiConversationsHistoryResponse =
            load_json_fixture_file("slack_fetch_message_response.json");
        let channel_response: SlackApiConversationsInfoResponse =
            load_json_fixture_file("slack_fetch_channel_response.json");
        let user_response: SlackApiUsersInfoResponse =
            load_json_fixture_file("slack_fetch_user_response.json");
        let sender = SlackMessageSenderDetails::User(Box::new(user_response.user));
        let team_response: SlackApiTeamInfoResponse =
            load_json_fixture_file("slack_fetch_team_response.json");

        Box::new(SlackStarItem::SlackMessage(SlackMessageDetails {
            url: "https://example.com".parse().unwrap(),
            message: message_response.messages[0].clone(),
            channel: channel_response.channel,
            sender,
            team: team_response.team,
            references: None,
        }))
    }

    mod test_message_content {
        use super::*;

        #[rstest]
        fn test_render_starred_message_with_blocks(slack_starred_message: Box<SlackStarItem>) {
            assert_eq!(
            slack_starred_message.content(),
            "🔴  *Test title* 🔴\n\n- list 1\n- list 2\n1. number 1\n1. number 2\n> quote\n```$ echo Hello world```\n_Some_ `formatted` ~text~.\n\nHere is a [link](https://www.universal-inbox.com)"
        );
        }

        #[rstest]
        fn test_render_starred_message_with_blocks_in_attachments(
            mut slack_starred_message: Box<SlackStarItem>,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut (*slack_starred_message) else {
                panic!(
                    "Expected SlackStarItem::SlackMessage, got {:?}",
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
        fn test_render_starred_message_with_text_in_attachments(
            mut slack_starred_message: Box<SlackStarItem>,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut (*slack_starred_message) else {
                panic!(
                    "Expected SlackStarItem::SlackMessage, got {:?}",
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
                text: Some("This is the text".to_string()),
                blocks: None,
            }]);
            message.message.content.blocks = Some(vec![]);
            assert_eq!(slack_starred_message.content(), "This is the text");
        }

        #[rstest]
        fn test_render_starred_message_with_text_and_title_in_attachments(
            mut slack_starred_message: Box<SlackStarItem>,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut (*slack_starred_message) else {
                panic!(
                    "Expected SlackStarItem::SlackMessage, got {:?}",
                    slack_starred_message
                );
            };
            message.message.content.attachments = Some(vec![SlackMessageAttachment {
                id: None,
                color: None,
                fallback: None,
                title: Some("This is the title".to_string()),
                fields: None,
                mrkdwn_in: None,
                text: Some("This is the text".to_string()),
                blocks: None,
            }]);
            message.message.content.blocks = Some(vec![]);
            assert_eq!(
                slack_starred_message.content(),
                "This is the title\n\nThis is the text"
            );
        }

        #[rstest]
        fn test_render_starred_message_with_only_text(
            mut slack_starred_message: Box<SlackStarItem>,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut (*slack_starred_message) else {
                panic!(
                    "Expected SlackStarItem::SlackMessage, got {:?}",
                    slack_starred_message
                );
            };
            message.message.content.text = Some("Test message".to_string());
            message.message.content.blocks = Some(vec![]);
            assert_eq!(slack_starred_message.content(), "Test message".to_string());
        }
    }

    mod test_message_title {
        use std::collections::HashMap;

        use super::*;

        #[rstest]
        fn test_render_starred_message_with_blocks(slack_starred_message: Box<SlackStarItem>) {
            assert_eq!(slack_starred_message.title(), "🔴  Test title 🔴...");
        }

        #[rstest]
        fn test_render_starred_message_with_text_in_attachments(
            mut slack_starred_message: Box<SlackStarItem>,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut (*slack_starred_message) else {
                panic!(
                    "Expected SlackStarItem::SlackMessage, got {:?}",
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
                text: Some("This is the text".to_string()),
                blocks: None,
            }]);
            message.message.content.blocks = Some(vec![]);
            assert_eq!(slack_starred_message.title(), "This is the text");
        }

        #[rstest]
        fn test_render_starred_message_with_text_and_title_in_attachments(
            mut slack_starred_message: Box<SlackStarItem>,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut (*slack_starred_message) else {
                panic!(
                    "Expected SlackStarItem::SlackMessage, got {:?}",
                    slack_starred_message
                );
            };
            message.message.content.attachments = Some(vec![SlackMessageAttachment {
                id: None,
                color: None,
                fallback: None,
                title: Some("This is the title".to_string()),
                fields: None,
                mrkdwn_in: None,
                text: Some("This is the text".to_string()),
                blocks: None,
            }]);
            message.message.content.blocks = Some(vec![]);
            assert_eq!(slack_starred_message.title(), "This is the title");
        }

        #[rstest]
        fn test_render_starred_message_with_blocks_in_attachments(
            mut slack_starred_message: Box<SlackStarItem>,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut (*slack_starred_message) else {
                panic!(
                    "Expected SlackStarItem::SlackMessage, got {:?}",
                    slack_starred_message
                );
            };
            message.message.content.blocks = Some(vec![SlackBlock::RichText(serde_json::json!({
                "type": "rich_text",
                "elements": [
                    {
                        "type": "rich_text_section",
                        "elements": [
                            {
                                "type": "user",
                                "user_id": "user1"
                            }
                        ]
                    },
                    {
                        "type": "rich_text_section",
                        "elements": [
                            {
                                "type": "user",
                                "user_id": "user2"
                            }
                        ]
                    },
                    {
                        "type": "rich_text_section",
                        "elements": [
                            {
                                "type": "usergroup",
                                "usergroup_id": "group1"
                            }
                        ]
                    },
                    {
                        "type": "rich_text_section",
                        "elements": [
                            {
                                "type": "usergroup",
                                "usergroup_id": "group2"
                            }
                        ]
                    },
                    {
                        "type": "rich_text_section",
                        "elements": [
                            {
                                "type": "channel",
                                "channel_id": "C0123456"
                            }
                        ]
                    },
                    {
                        "type": "rich_text_section",
                        "elements": [
                            {
                                "type": "channel",
                                "channel_id": "C0011223"
                            }
                        ]
                    },
                ]
            }))]);
            message.references = Some(SlackReferences {
                users: HashMap::from([(
                    SlackUserId("user1".to_string()),
                    Some("John Doe".to_string()),
                )]),
                channels: HashMap::from([(
                    SlackChannelId("C0123456".to_string()),
                    Some("general".to_string()),
                )]),
                usergroups: HashMap::from([(
                    SlackUserGroupId("group1".to_string()),
                    Some("Admins".to_string()),
                )]),
            });
            assert_eq!(
                slack_starred_message.content(),
                "@John Doe@user2@Admins@group2#general#C0011223"
            );
        }
    }
}
