use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use slack_blocks_render::render_blocks_as_markdown;
use slack_morphism::prelude::*;
use url::Url;
use uuid::Uuid;

use crate::{
    notification::{Notification, NotificationMetadata, NotificationStatus},
    user::UserId,
    utils::{emoji::replace_emoji_code_in_string_with_emoji, truncate::truncate_with_ellipse},
    HasHtmlUrl,
};

pub trait SlackPushEventCallbackExt {
    fn into_notification(self, user_id: UserId) -> Result<Notification>;
}

impl SlackPushEventCallbackExt for SlackPushEventCallback {
    fn into_notification(self, user_id: UserId) -> Result<Notification> {
        let (updated_at, source_id, title, blocks, status) = match &self {
            SlackPushEventCallback {
                event_time,
                event:
                    SlackEventCallbackBody::StarAdded(SlackStarAddedEvent {
                        item:
                            SlackStarsItem::Message(SlackStarsItemMessage {
                                message:
                                    SlackHistoryMessage {
                                        origin: SlackMessageOrigin { ts, .. },
                                        content: SlackMessageContent { text, blocks, .. },
                                        ..
                                    },
                                ..
                            }),
                        ..
                    }),
                ..
            } => (
                event_time.0,
                ts.to_string(),
                (*text).clone().unwrap_or("Starred message".to_string()),
                blocks,
                NotificationStatus::Unread,
            ),
            SlackPushEventCallback {
                event_time,
                event:
                    SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent {
                        item:
                            SlackStarsItem::Message(SlackStarsItemMessage {
                                message:
                                    SlackHistoryMessage {
                                        origin: SlackMessageOrigin { ts, .. },
                                        content: SlackMessageContent { text, blocks, .. },
                                        ..
                                    },
                                ..
                            }),
                        ..
                    }),
                ..
            } => (
                event_time.0,
                ts.to_string(),
                (*text).clone().unwrap_or("Starred message".to_string()),
                blocks,
                NotificationStatus::Deleted,
            ),
            SlackPushEventCallback {
                event_time,
                event:
                    SlackEventCallbackBody::ReactionAdded(SlackReactionAddedEvent {
                        item:
                            SlackReactionsItem::Message(SlackHistoryMessage {
                                origin: SlackMessageOrigin { ts, .. },
                                content: SlackMessageContent { text, blocks, .. },
                                ..
                            }),
                        ..
                    }),
                ..
            } => (
                event_time.0,
                ts.to_string(),
                (*text).clone().unwrap_or("Reaction on message".to_string()),
                blocks,
                NotificationStatus::Unread,
            ),
            SlackPushEventCallback {
                event_time,
                event:
                    SlackEventCallbackBody::ReactionRemoved(SlackReactionRemovedEvent {
                        item:
                            SlackReactionsItem::Message(SlackHistoryMessage {
                                origin: SlackMessageOrigin { ts, .. },
                                content: SlackMessageContent { text, blocks, .. },
                                ..
                            }),
                        ..
                    }),
                ..
            } => (
                event_time.0,
                ts.to_string(),
                (*text).clone().unwrap_or("Reaction on message".to_string()),
                blocks,
                NotificationStatus::Deleted,
            ),
            _ => return Err(anyhow!("Unsupported Slack event {self:?}")),
        };
        let content_with_emojis = if let Some(blocks) = &blocks {
            render_blocks_as_markdown(blocks.clone())
        } else {
            replace_emoji_code_in_string_with_emoji(&title)
        };
        let title = truncate_with_ellipse(&content_with_emojis, 50, "...", true);

        Ok(Notification {
            id: Uuid::new_v4().into(),
            title,
            source_id,
            status,
            metadata: NotificationMetadata::Slack(Box::new(self.clone())),
            updated_at,
            last_read_at: None,
            snoozed_until: None,
            user_id,
            details: None,
            task_id: None,
        })
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackMessageDetails {
    pub url: Url,
    pub message: SlackHistoryMessage,
    pub channel: SlackChannelInfo,
    pub sender: SlackMessageSenderDetails,
    pub team: SlackTeamInfo,
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

    pub fn content(&self) -> String {
        if let Some(blocks) = &self.message.content.blocks {
            if !blocks.is_empty() {
                return render_blocks_as_markdown(blocks.clone());
            }
        }

        if let Some(attachments) = &self.message.content.attachments {
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
mod tests {
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
