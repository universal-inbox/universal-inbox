use anyhow::{anyhow, Result};
use slack_blocks_render::{render_blocks_as_markdown, SlackReferences};
use slack_morphism::prelude::*;
use uuid::Uuid;

use crate::{
    notification::{Notification, NotificationMetadata, NotificationStatus},
    user::UserId,
    utils::{emoji::replace_emoji_code_in_string_with_emoji, truncate::truncate_with_ellipse},
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
            render_blocks_as_markdown(blocks.clone(), SlackReferences::default())
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
