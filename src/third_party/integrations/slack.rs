use std::collections::HashMap;

use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use slack_blocks_render::{
    render_blocks_as_markdown, text::render_blocks_as_text, SlackReferences,
};
use slack_morphism::prelude::*;
use url::Url;
use uuid::Uuid;
use vec1::Vec1;

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
    SlackMessage(Box<SlackMessageDetails>),
    SlackFile(Box<SlackFileDetails>),
    SlackFileComment(Box<SlackFileCommentDetails>),
    SlackChannel(Box<SlackChannelDetails>),
    SlackIm(Box<SlackImDetails>),
    SlackGroup(Box<SlackGroupDetails>),
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

    pub fn team_id(&self) -> SlackTeamId {
        match self {
            SlackStarItem::SlackMessage(message) => message.team.id.clone(),
            SlackStarItem::SlackFile(file) => file.team.id.clone(),
            SlackStarItem::SlackFileComment(comment) => comment.team.id.clone(),
            SlackStarItem::SlackChannel(channel) => channel.team.id.clone(),
            SlackStarItem::SlackIm(im) => im.team.id.clone(),
            SlackStarItem::SlackGroup(group) => group.team.id.clone(),
        }
    }

    pub fn render_title(&self) -> String {
        match self {
            SlackStarItem::SlackMessage(message) => message.render_title(),
            SlackStarItem::SlackFile(file) => file.render_title(),
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

    pub fn render_content(&self) -> String {
        match self {
            SlackStarItem::SlackMessage(message) => message.render_content(),
            SlackStarItem::SlackFile(file) => file.render_content(),
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
            SlackStarItem::SlackMessage(slack_message) => (
                Some(slack_message.channel.id.clone()),
                Some(slack_message.message.origin.ts.clone()),
                None,
                None,
            ),
            SlackStarItem::SlackChannel(slack_channel) => {
                (Some(slack_channel.channel.id.clone()), None, None, None)
            }
            SlackStarItem::SlackIm(slack_im) => {
                (Some(slack_im.channel.id.clone()), None, None, None)
            }
            SlackStarItem::SlackGroup(slack_group) => {
                (Some(slack_group.channel.id.clone()), None, None, None)
            }
            SlackStarItem::SlackFile(slack_file) => (
                Some(slack_file.channel.id.clone()),
                None,
                slack_file.id.clone(),
                None,
            ),
            SlackStarItem::SlackFileComment(slack_file_comment) => (
                Some(slack_file_comment.channel.id.clone()),
                None,
                None,
                Some(slack_file_comment.comment_id.clone()),
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
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::SlackStar(slack_star) => Ok(*slack_star),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to SlackStar",
                item.id
            )),
        }
    }
}

impl ThirdPartyItemFromSource for SlackStar {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.item.id(),
            data: ThirdPartyItemData::SlackStar(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
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

    pub fn team_id(&self) -> SlackTeamId {
        match self {
            SlackReactionItem::SlackMessage(message) => message.team.id.clone(),
            SlackReactionItem::SlackFile(file) => file.team.id.clone(),
        }
    }

    pub fn render_title(&self) -> String {
        match self {
            SlackReactionItem::SlackMessage(message) => message.render_title(),
            SlackReactionItem::SlackFile(file) => file.render_title(),
        }
    }

    pub fn render_content(&self) -> String {
        match self {
            SlackReactionItem::SlackMessage(message) => message.render_content(),
            SlackReactionItem::SlackFile(file) => file.render_content(),
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
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::SlackReaction(slack_reaction) => Ok(*slack_reaction),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to SlackReaction",
                item.id
            )),
        }
    }
}

impl ThirdPartyItemFromSource for SlackReaction {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.item.id(),
            data: ThirdPartyItemData::SlackReaction(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
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

pub trait SlackMessageRender {
    fn render_content(&self, references: Option<SlackReferences>, as_markdown: bool) -> String;
    fn render_title(&self, references: Option<SlackReferences>) -> String;
    fn get_sender(
        &self,
        sender_profiles: &HashMap<String, SlackMessageSenderDetails>,
    ) -> Option<SlackMessageSenderDetails>;
}

impl SlackMessageRender for SlackHistoryMessage {
    fn render_content(&self, references: Option<SlackReferences>, as_markdown: bool) -> String {
        if let Some(blocks) = &self.content.blocks {
            if !blocks.is_empty() {
                return if as_markdown {
                    render_blocks_as_markdown(
                        blocks.clone(),
                        references.clone().unwrap_or_default(),
                        Some("@".to_string()),
                    )
                } else {
                    render_blocks_as_text(blocks.clone(), references.clone().unwrap_or_default())
                };
            }
        }

        if let Some(attachments) = &self.content.attachments {
            if !attachments.is_empty() {
                let str_blocks = attachments
                    .iter()
                    .filter_map(|a| {
                        if let Some(blocks) = a.blocks.as_ref() {
                            return if as_markdown {
                                Some(render_blocks_as_markdown(
                                    blocks.clone(),
                                    references.clone().unwrap_or_default(),
                                    Some("@".to_string()),
                                ))
                            } else {
                                Some(render_blocks_as_text(
                                    blocks.clone(),
                                    references.clone().unwrap_or_default(),
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

        let message = match &self.content.text {
            Some(text) => sanitize_slack_markdown(text),
            _ => "A slack message".to_string(),
        };

        replace_emoji_code_in_string_with_emoji(&message)
    }

    fn render_title(&self, references: Option<SlackReferences>) -> String {
        if let Some(attachments) = &self.content.attachments {
            if let Some(first_attachment) = attachments.first() {
                if let Some(title) = first_attachment.title.as_ref() {
                    return title.clone();
                }
            }
        }

        truncate_with_ellipse(&self.render_content(references, false), 120, "...", true)
    }

    fn get_sender(
        &self,
        sender_profiles: &HashMap<String, SlackMessageSenderDetails>,
    ) -> Option<SlackMessageSenderDetails> {
        if let Some(user_profile) = self.sender.user_profile.as_ref() {
            return Some(SlackMessageSenderDetails::User(Box::new(
                user_profile.clone(),
            )));
        }
        if let Some(bot_profile) = self.sender.bot_profile.as_ref() {
            return Some(SlackMessageSenderDetails::Bot(Box::new(
                bot_profile.clone(),
            )));
        }
        let sender_id = match self.sender {
            SlackMessageSender {
                user: Some(ref user_id),
                ..
            } => Some(user_id.to_string()),
            SlackMessageSender {
                bot_id: Some(ref bot_id),
                ..
            } => Some(bot_id.to_string()),
            _ => None,
        };
        sender_id.and_then(|id| sender_profiles.get(&id).cloned())
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

    pub fn render_title(&self) -> String {
        if let Some(attachments) = &self.message.content.attachments {
            if let Some(first_attachment) = attachments.first() {
                if let Some(title) = first_attachment.title.as_ref() {
                    return title.clone();
                }
            }
        }

        truncate_with_ellipse(
            &self.message.render_content(self.references.clone(), false),
            120,
            "...",
            true,
        )
    }

    pub fn render_content(&self) -> String {
        self.message.render_content(self.references.clone(), true)
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
    User(Box<SlackUserProfile>),
    Bot(Box<SlackBotInfo>),
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackFileDetails {
    pub id: Option<SlackFileId>, // Option to ease the transition when the field is added
    pub title: Option<String>,
    pub channel: SlackChannelInfo,
    pub sender: Option<SlackUserProfile>,
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
    pub fn render_title(&self) -> String {
        self.title.clone().unwrap_or_else(|| "File".to_string())
    }

    pub fn render_content(&self) -> String {
        self.title.clone().unwrap_or_else(|| "File".to_string())
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackFileCommentDetails {
    pub channel: SlackChannelInfo,
    pub comment_id: SlackFileCommentId,
    pub sender: Option<SlackUserProfile>,
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

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackThread {
    pub url: Url,
    pub messages: Vec1<SlackHistoryMessage>,
    pub sender_profiles: HashMap<String, SlackMessageSenderDetails>,
    pub subscribed: bool,
    pub last_read: Option<SlackTs>,
    pub channel: SlackChannelInfo,
    pub team: SlackTeamInfo,
    pub references: Option<SlackReferences>,
}

impl SlackThread {
    pub fn get_channel_html_url(&self) -> Url {
        format!(
            "https://app.slack.com/client/{}/{}",
            self.team.id, self.channel.id
        )
        .parse()
        .unwrap()
    }

    pub fn first_unread_message_from_last_read<'a>(
        last_read: &'a Option<SlackTs>,
        messages: &'a Vec1<SlackHistoryMessage>,
    ) -> &'a SlackHistoryMessage {
        let Some(last_read) = &last_read else {
            return messages.first();
        };

        let message_index = messages
            .iter()
            .position(|m| m.origin.ts == *last_read)
            .map(|i| i + 1)
            .unwrap_or(0);
        messages
            .get(message_index)
            .unwrap_or_else(|| messages.last())
    }

    pub fn first_unread_message(&self) -> &SlackHistoryMessage {
        SlackThread::first_unread_message_from_last_read(&self.last_read, &self.messages)
    }

    pub fn render_title(&self) -> String {
        self.first_unread_message()
            .render_title(self.references.clone())
    }
}

impl HasHtmlUrl for SlackThread {
    fn get_html_url(&self) -> Url {
        self.url.clone()
    }
}

impl TryFrom<ThirdPartyItem> for SlackThread {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::SlackThread(slack_thread) => Ok(*slack_thread),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to SlackThread",
                item.id
            )),
        }
    }
}

impl ThirdPartyItemFromSource for SlackThread {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        let first_message = self.messages.first();
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: first_message.origin.ts.to_string(),
            data: ThirdPartyItemData::SlackThread(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
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
    use rstest::*;
    use slack_morphism::{
        api::{
            SlackApiConversationsHistoryResponse, SlackApiConversationsInfoResponse,
            SlackApiTeamInfoResponse, SlackApiUsersInfoResponse,
        },
        SlackMessageAttachment,
    };

    use crate::test_helpers::load_json_fixture_file;

    use super::*;

    #[fixture]
    pub fn slack_starred_message() -> SlackStarItem {
        let message_response: SlackApiConversationsHistoryResponse =
            load_json_fixture_file("slack_fetch_message_response.json");
        let channel_response: SlackApiConversationsInfoResponse =
            load_json_fixture_file("slack_fetch_channel_response.json");
        let user_response: SlackApiUsersInfoResponse =
            load_json_fixture_file("slack_fetch_user_response.json");
        let sender = SlackMessageSenderDetails::User(Box::new(user_response.user.profile.unwrap()));
        let team_response: SlackApiTeamInfoResponse =
            load_json_fixture_file("slack_fetch_team_response.json");

        SlackStarItem::SlackMessage(Box::new(SlackMessageDetails {
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
        fn test_render_starred_message_with_blocks(slack_starred_message: SlackStarItem) {
            assert_eq!(
                slack_starred_message.render_content(),
                r#"📥  *Universal Inbox new release* 📥
- list 1
- list 2

1. number 1
1. number 2

> quote


```
$ echo Hello world
```
\
_Some_ `formatted` ~text~.\
\
Here is a [link](https://www.universal-inbox.com)"#
            );
        }

        #[rstest]
        fn test_render_starred_message_with_blocks_in_attachments(
            mut slack_starred_message: SlackStarItem,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut slack_starred_message else {
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
                slack_starred_message.render_content(),
                r#"📥  *Universal Inbox new release* 📥
- list 1
- list 2

1. number 1
1. number 2

> quote


```
$ echo Hello world
```
\
_Some_ `formatted` ~text~.\
\
Here is a [link](https://www.universal-inbox.com)"#
            );
        }

        #[rstest]
        fn test_render_starred_message_with_text_in_attachments(
            mut slack_starred_message: SlackStarItem,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut slack_starred_message else {
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
            assert_eq!(slack_starred_message.render_content(), "This is the text");
        }

        #[rstest]
        fn test_render_starred_message_with_text_and_title_in_attachments(
            mut slack_starred_message: SlackStarItem,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut slack_starred_message else {
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
                slack_starred_message.render_content(),
                "This is the title\n\nThis is the text"
            );
        }

        #[rstest]
        fn test_render_starred_message_with_only_text(mut slack_starred_message: SlackStarItem) {
            let SlackStarItem::SlackMessage(message) = &mut slack_starred_message else {
                panic!(
                    "Expected SlackStarItem::SlackMessage, got {:?}",
                    slack_starred_message
                );
            };
            message.message.content.text = Some("Test message".to_string());
            message.message.content.blocks = Some(vec![]);
            assert_eq!(
                slack_starred_message.render_content(),
                "Test message".to_string()
            );
        }
    }

    mod test_message_title {
        use std::collections::HashMap;

        use super::*;

        #[rstest]
        fn test_render_starred_message_with_blocks(slack_starred_message: SlackStarItem) {
            assert_eq!(
                slack_starred_message.render_title(),
                "📥  Universal Inbox new release 📥..."
            );
        }

        #[rstest]
        fn test_render_starred_message_with_text_in_attachments(
            mut slack_starred_message: SlackStarItem,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut slack_starred_message else {
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
            assert_eq!(slack_starred_message.render_title(), "This is the text");
        }

        #[rstest]
        fn test_render_starred_message_with_text_and_title_in_attachments(
            mut slack_starred_message: SlackStarItem,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut slack_starred_message else {
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
            assert_eq!(slack_starred_message.render_title(), "This is the title");
        }

        #[rstest]
        fn test_render_starred_message_with_blocks_in_attachments(
            mut slack_starred_message: SlackStarItem,
        ) {
            let SlackStarItem::SlackMessage(message) = &mut slack_starred_message else {
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
                    {
                        "type": "rich_text_section",
                        "elements": [
                            {
                                "type": "emoji",
                                "name": "unknown1"
                            }
                        ]
                    },
                    {
                        "type": "rich_text_section",
                        "elements": [
                            {
                                "type": "emoji",
                                "name": "unknown2"
                            }
                        ]
                    }
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
                emojis: HashMap::from([
                    (
                        SlackEmojiName("unknown1".to_string()),
                        Some(SlackEmojiRef::Alias(SlackEmojiName("wave".to_string()))),
                    ),
                    (
                        SlackEmojiName("unknown2".to_string()),
                        Some(SlackEmojiRef::Url(
                            "https://emoji.com/unknown2.png".parse().unwrap(),
                        )),
                    ),
                ]),
            });
            assert_eq!(
                slack_starred_message.render_content(),
                "@@John Doe@\n@@user2@\n@@Admins@\n@@group2@\n#general\n#C0011223\n👋\n![:unknown2:](https://emoji.com/unknown2.png)"
            );
        }
    }

    mod test_thread {
        use super::*;

        #[fixture]
        pub fn slack_thread() -> Box<SlackThread> {
            let message_response: SlackApiConversationsHistoryResponse =
                load_json_fixture_file("slack_fetch_thread_response.json");
            let channel_response: SlackApiConversationsInfoResponse =
                load_json_fixture_file("slack_fetch_channel_response.json");
            let team_response: SlackApiTeamInfoResponse =
                load_json_fixture_file("slack_fetch_team_response.json");

            Box::new(SlackThread {
                url: "https://example.com".parse().unwrap(),
                messages: message_response.messages.try_into().unwrap(),
                subscribed: true,
                last_read: None,
                channel: channel_response.channel.clone(),
                team: team_response.team.clone(),
                references: None,
                sender_profiles: Default::default(),
            })
        }

        mod render_title {
            use super::*;

            #[rstest]
            fn test_render_thread_with_only_unread_message(mut slack_thread: Box<SlackThread>) {
                slack_thread.last_read = None;
                assert_eq!(slack_thread.render_title(), "Hello"); // ie. first message
            }

            #[rstest]
            fn test_render_thread_with_first_message_read(mut slack_thread: Box<SlackThread>) {
                slack_thread.last_read = Some(slack_thread.messages.first().origin.ts.clone());
                assert_eq!(slack_thread.render_title(), "World"); // ie. second message
            }

            #[rstest]
            fn test_render_thread_with_all_messages_read(mut slack_thread: Box<SlackThread>) {
                slack_thread.last_read = Some(slack_thread.messages.last().origin.ts.clone());
                assert_eq!(slack_thread.render_title(), "World"); // ie. second and last message
            }

            #[rstest]
            fn test_render_thread_with_unknown_message_read(mut slack_thread: Box<SlackThread>) {
                slack_thread.last_read = Some(SlackTs("unknown".to_string()));
                assert_eq!(slack_thread.render_title(), "Hello"); // ie. first message
            }
        }

        mod render_content {
            use super::*;

            #[rstest]
            fn test_render_thread_with_reference_custom_emoji(mut slack_thread: Box<SlackThread>) {
                let first_message = slack_thread.messages.first_mut();
                first_message.content.blocks =
                    Some(vec![SlackBlock::RichText(serde_json::json!({
                        "type": "rich_text",
                        "elements": [
                            {
                                "type": "rich_text_section",
                                "elements": [
                                    {
                                        "text": "Hello ",
                                        "type": "text"
                                    },
                                    {
                                        "type": "emoji",
                                        "name": "custom_emoji"
                                    }
                                ]
                            }
                        ]
                    }))]);
                let slack_references = SlackReferences {
                    users: Default::default(),
                    channels: Default::default(),
                    usergroups: Default::default(),
                    emojis: HashMap::from([(
                        SlackEmojiName("custom_emoji".to_string()),
                        Some(SlackEmojiRef::Url(
                            "https://emoji.com/custom_emoji.png".parse().unwrap(),
                        )),
                    )]),
                };

                assert_eq!(
                    first_message.render_content(Some(slack_references), true),
                    "Hello ![:custom_emoji:](https://emoji.com/custom_emoji.png)"
                );
            }
        }
    }
}
