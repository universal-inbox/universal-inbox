use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
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
        let (updated_at, source_id, title, status) = match &self {
            SlackPushEventCallback {
                event_time,
                event:
                    SlackEventCallbackBody::StarAdded(SlackStarAddedEvent {
                        item:
                            SlackStarsItem::Message(SlackStarsItemMessage {
                                message:
                                    SlackHistoryMessage {
                                        origin: SlackMessageOrigin { ts, .. },
                                        content: SlackMessageContent { text, .. },
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
                                        content: SlackMessageContent { text, .. },
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
                NotificationStatus::Deleted,
            ),
            _ => return Err(anyhow!("Unsupported Slack event {self:?}")),
        };
        let title_with_emojis = replace_emoji_code_in_string_with_emoji(&title);
        let title = truncate_with_ellipse(&title_with_emojis, 50, "...");

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

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
#[serde(tag = "type", content = "content")]
pub enum SlackMessageSenderDetails {
    User(Box<SlackUser>),
    Bot(SlackBotInfo),
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackFileDetails {
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

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct SlackFileCommentDetails {
    pub channel: SlackChannelInfo,
    pub comment: String,
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
