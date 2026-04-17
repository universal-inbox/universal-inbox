use std::collections::HashMap;

use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use slack_blocks_render::{
    SlackReferences, html::render_blocks_as_html, render_blocks_as_markdown,
    render_slack_mrkdwn_text_as_html, text::render_blocks_as_text,
};
use slack_morphism::prelude::*;
use url::Url;
use uuid::Uuid;
use vec1::Vec1;

use crate::{
    HasHtmlUrl,
    integration_connection::IntegrationConnectionId,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
    utils::{emoji::replace_emoji_code_in_string_with_emoji, truncate::truncate_with_ellipse},
};

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
            source_id: self.source_id(),
            data: ThirdPartyItemData::SlackReaction(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }

    fn source_id(&self) -> String {
        self.item.id()
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
    fn render_content_as_html(
        &self,
        references: Option<SlackReferences>,
        default_style_class: &str,
        highlight_style_class: &str,
        user_slack_id: Option<String>,
    ) -> String;
    fn render_title(&self, references: Option<SlackReferences>) -> String;
    fn get_sender(
        &self,
        sender_profiles: &HashMap<String, SlackMessageSenderDetails>,
    ) -> Option<SlackMessageSenderDetails>;
}

impl SlackMessageRender for SlackHistoryMessage {
    fn render_content(&self, references: Option<SlackReferences>, as_markdown: bool) -> String {
        if let Some(blocks) = &self.content.blocks
            && !blocks.is_empty()
        {
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

        if let Some(attachments) = &self.content.attachments
            && !attachments.is_empty()
        {
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

        let message = match &self.content.text {
            Some(text) => sanitize_slack_markdown(text),
            _ => "A slack message".to_string(),
        };

        replace_emoji_code_in_string_with_emoji(&message)
    }

    fn render_content_as_html(
        &self,
        references: Option<SlackReferences>,
        default_style_class: &str,
        highlight_style_class: &str,
        user_slack_id: Option<String>,
    ) -> String {
        let references = references.map(|mut refs| {
            refs.user_id_to_highlight = user_slack_id.map(SlackUserId);
            refs
        });

        if let Some(blocks) = &self.content.blocks
            && !blocks.is_empty()
        {
            return render_blocks_as_html(
                blocks.clone(),
                references.clone().unwrap_or_default(),
                default_style_class,
                highlight_style_class,
            );
        }

        if let Some(attachments) = &self.content.attachments
            && !attachments.is_empty()
        {
            let str_blocks = attachments
                .iter()
                .filter_map(|a| {
                    if let Some(blocks) = a.blocks.as_ref() {
                        return Some(render_blocks_as_html(
                            blocks.clone(),
                            references.clone().unwrap_or_default(),
                            default_style_class,
                            highlight_style_class,
                        ));
                    }

                    if let Some(text) = a.text.as_ref() {
                        let rendered_text = render_slack_mrkdwn_text_as_html(
                            text,
                            &references.clone().unwrap_or_default(),
                            default_style_class,
                            highlight_style_class,
                        );
                        if let Some(title) = a.title.as_ref() {
                            let escaped_title = html_escape::encode_text(title);
                            return Some(format!(
                                "<p>{escaped_title}</p>\n<p>{rendered_text}</p>\n",
                            ));
                        }

                        return Some(format!("<p>{rendered_text}</p>\n"));
                    }

                    None
                })
                .collect::<Vec<String>>();

            if !str_blocks.is_empty() {
                return str_blocks.join("");
            }
        }

        let text = match &self.content.text {
            Some(text) => text.as_str(),
            _ => "A slack message",
        };

        format!(
            "<p>{}</p>\n",
            render_slack_mrkdwn_text_as_html(
                text,
                &references.unwrap_or_default(),
                default_style_class,
                highlight_style_class,
            )
        )
    }

    fn render_title(&self, references: Option<SlackReferences>) -> String {
        if let Some(attachments) = &self.content.attachments
            && let Some(first_attachment) = attachments.first()
            && let Some(title) = first_attachment.title.as_ref()
        {
            return title.clone();
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
        if let Some(attachments) = &self.message.content.attachments
            && let Some(first_attachment) = attachments.first()
            && let Some(title) = first_attachment.title.as_ref()
        {
            return title.clone();
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

    pub fn render_content_as_html(
        &self,
        default_style_class: &str,
        highlight_style_class: &str,
        user_slack_id: Option<String>,
    ) -> String {
        self.message.render_content_as_html(
            self.references.clone(),
            default_style_class,
            highlight_style_class,
            user_slack_id,
        )
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
    /// The Slack user ID of the current user (from IntegrationConnection.provider_user_id)
    #[serde(default)]
    pub user_slack_id: Option<String>,
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

    /// Check if the last message in the thread was sent by the current user
    pub fn is_last_message_from_user(&self) -> bool {
        let Some(ref user_slack_id) = self.user_slack_id else {
            return false;
        };

        let last_message = self.messages.last();

        // Check if the sender is a user (not a bot) and matches the current user
        match &last_message.sender {
            SlackMessageSender {
                user: Some(sender_user_id),
                ..
            } => sender_user_id.to_string() == *user_slack_id,
            _ => false,
        }
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
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.source_id(),
            data: ThirdPartyItemData::SlackThread(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }

    fn source_id(&self) -> String {
        let first_message = self.messages.first();
        first_message.origin.ts.to_string()
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
        SlackMessageAttachment,
        api::{
            SlackApiConversationsHistoryResponse, SlackApiConversationsInfoResponse,
            SlackApiTeamInfoResponse, SlackApiUsersInfoResponse,
        },
    };

    use crate::test_helpers::load_json_fixture_file;

    use super::*;

    #[fixture]
    pub fn slack_message() -> SlackMessageDetails {
        let message_response: SlackApiConversationsHistoryResponse =
            load_json_fixture_file("slack_fetch_message_response.json");
        let channel_response: SlackApiConversationsInfoResponse =
            load_json_fixture_file("slack_fetch_channel_response.json");
        let user_response: SlackApiUsersInfoResponse =
            load_json_fixture_file("slack_fetch_user_response.json");
        let sender = SlackMessageSenderDetails::User(Box::new(user_response.user.profile.unwrap()));
        let team_response: SlackApiTeamInfoResponse =
            load_json_fixture_file("slack_fetch_team_response.json");

        SlackMessageDetails {
            url: "https://example.com".parse().unwrap(),
            message: message_response.messages[0].clone(),
            channel: channel_response.channel,
            sender,
            team: team_response.team,
            references: None,
        }
    }

    mod test_message_content {
        use super::*;

        #[rstest]
        fn test_render_message_with_blocks(slack_message: SlackMessageDetails) {
            assert_eq!(
                slack_message.render_content(),
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
Here is a [link](https://www.universal-inbox.com/)"#
            );
        }

        #[rstest]
        fn test_render_message_with_blocks_in_attachments(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
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
                slack_message.render_content(),
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
Here is a [link](https://www.universal-inbox.com/)"#
            );
        }

        #[rstest]
        fn test_render_message_with_text_in_attachments(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
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
            assert_eq!(slack_message.render_content(), "This is the text");
        }

        #[rstest]
        fn test_render_message_with_text_and_title_in_attachments(
            mut slack_message: SlackMessageDetails,
        ) {
            let message = &mut slack_message;
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
                slack_message.render_content(),
                "This is the title\n\nThis is the text"
            );
        }

        #[rstest]
        fn test_render_message_with_only_text(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
            message.message.content.text = Some("Test message".to_string());
            message.message.content.blocks = Some(vec![]);
            assert_eq!(slack_message.render_content(), "Test message".to_string());
        }
    }

    mod test_message_content_as_html {
        use super::*;

        fn render_html(slack_message: &SlackMessageDetails) -> String {
            slack_message.render_content_as_html("text-primary", "text-warning", None)
        }

        #[rstest]
        fn test_render_message_with_blocks(slack_message: SlackMessageDetails) {
            let html = render_html(&slack_message);
            // Blocks path delegates to render_blocks_as_html — verify it produces HTML tags
            assert!(html.contains("<strong>"), "Expected HTML bold tag: {html}");
            assert!(html.contains("<a "), "Expected HTML link tag: {html}");
        }

        #[rstest]
        fn test_render_attachment_text_with_slack_link(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
            message.message.content.attachments = Some(vec![SlackMessageAttachment {
                id: None,
                color: None,
                fallback: None,
                title: None,
                fields: None,
                mrkdwn_in: None,
                text: Some("Check <https://example.com|this link>".to_string()),
                blocks: None,
            }]);
            message.message.content.blocks = Some(vec![]);
            let html = render_html(&slack_message);
            assert!(
                html.contains(r#"<a target="_blank" rel="noopener noreferrer" href="https://example.com">this link</a>"#),
                "Slack link should render as HTML anchor: {html}"
            );
            assert!(
                !html.contains("[this link]"),
                "Should not contain raw Markdown link syntax: {html}"
            );
        }

        #[rstest]
        fn test_render_attachment_text_with_title(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
            message.message.content.attachments = Some(vec![SlackMessageAttachment {
                id: None,
                color: None,
                fallback: None,
                title: Some("The title".to_string()),
                fields: None,
                mrkdwn_in: None,
                text: Some("*bold text*".to_string()),
                blocks: None,
            }]);
            message.message.content.blocks = Some(vec![]);
            let html = render_html(&slack_message);
            assert!(
                html.contains("<p>The title</p>"),
                "Title should be in its own paragraph: {html}"
            );
            assert!(
                html.contains("<strong>bold text</strong>"),
                "Text should have bold rendered as HTML: {html}"
            );
        }

        #[rstest]
        fn test_render_plain_text_fallback_with_slack_link(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
            message.message.content.text = Some("Visit <https://example.com|our site>".to_string());
            message.message.content.blocks = Some(vec![]);
            let html = render_html(&slack_message);
            assert!(
                html.contains(r#"href="https://example.com">our site</a>"#),
                "Slack link should render as HTML anchor: {html}"
            );
        }

        #[rstest]
        fn test_render_plain_text_fallback_simple(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
            message.message.content.text = Some("Simple & plain".to_string());
            message.message.content.blocks = Some(vec![]);
            let html = render_html(&slack_message);
            assert_eq!(html, "<p>Simple &amp; plain</p>\n");
        }
    }

    mod test_message_title {
        use std::collections::HashMap;

        use super::*;

        #[rstest]
        fn test_render_message_with_blocks(slack_message: SlackMessageDetails) {
            assert_eq!(
                slack_message.render_title(),
                "📥  Universal Inbox new release 📥..."
            );
        }

        #[rstest]
        fn test_render_message_with_text_in_attachments(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
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
            assert_eq!(slack_message.render_title(), "This is the text");
        }

        #[rstest]
        fn test_render_message_with_text_and_title_in_attachments(
            mut slack_message: SlackMessageDetails,
        ) {
            let message = &mut slack_message;
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
            assert_eq!(slack_message.render_title(), "This is the title");
        }

        #[rstest]
        fn test_render_message_with_blocks_in_attachments(mut slack_message: SlackMessageDetails) {
            let message = &mut slack_message;
            message.message.content.blocks = Some(vec![SlackBlock::RichText(
                serde_json::from_value(serde_json::json!({
                    "type": "rich_text",
                    "elements": [
                        {
                            "type": "rich_text_section",
                            "elements": [
                                { "type": "user", "user_id": "user1" }
                            ]
                        },
                        {
                            "type": "rich_text_section",
                            "elements": [
                                { "type": "user", "user_id": "user2" }
                            ]
                        },
                        {
                            "type": "rich_text_section",
                            "elements": [
                                { "type": "usergroup", "usergroup_id": "group1" }
                            ]
                        },
                        {
                            "type": "rich_text_section",
                            "elements": [
                                { "type": "usergroup", "usergroup_id": "group2" }
                            ]
                        },
                        {
                            "type": "rich_text_section",
                            "elements": [
                                { "type": "channel", "channel_id": "C0123456" }
                            ]
                        },
                        {
                            "type": "rich_text_section",
                            "elements": [
                                { "type": "channel", "channel_id": "C0011223" }
                            ]
                        },
                        {
                            "type": "rich_text_section",
                            "elements": [
                                { "type": "emoji", "name": "unknown1" }
                            ]
                        },
                        {
                            "type": "rich_text_section",
                            "elements": [
                                { "type": "emoji", "name": "unknown2" }
                            ]
                        }
                    ]
                }))
                .unwrap(),
            )]);
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
                ..SlackReferences::default()
            });
            assert_eq!(
                slack_message.render_content(),
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
                user_slack_id: None,
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
                first_message.content.blocks = Some(vec![SlackBlock::RichText(
                    serde_json::from_value(serde_json::json!({
                        "type": "rich_text",
                        "elements": [
                            {
                                "type": "rich_text_section",
                                "elements": [
                                    { "text": "Hello ", "type": "text" },
                                    { "type": "emoji", "name": "custom_emoji" }
                                ]
                            }
                        ]
                    }))
                    .unwrap(),
                )]);
                let slack_references = SlackReferences {
                    emojis: HashMap::from([(
                        SlackEmojiName("custom_emoji".to_string()),
                        Some(SlackEmojiRef::Url(
                            "https://emoji.com/custom_emoji.png".parse().unwrap(),
                        )),
                    )]),
                    ..SlackReferences::default()
                };

                assert_eq!(
                    first_message.render_content(Some(slack_references), true),
                    "Hello ![:custom_emoji:](https://emoji.com/custom_emoji.png)"
                );
            }
        }
    }
}
