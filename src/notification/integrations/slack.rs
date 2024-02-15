use anyhow::{anyhow, Result};
use slack_morphism::prelude::*;
use uuid::Uuid;

use crate::{
    notification::{Notification, NotificationMetadata, NotificationStatus},
    user::UserId,
    utils::emoji::replace_emoji_code_in_string_with_emoji,
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
                            SlackStarsItem::Message {
                                message:
                                    SlackHistoryMessage {
                                        origin: SlackMessageOrigin { ts, .. },
                                        content: SlackMessageContent { text, .. },
                                        ..
                                    },
                                ..
                            },
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
                            SlackStarsItem::Message {
                                message:
                                    SlackHistoryMessage {
                                        origin: SlackMessageOrigin { ts, .. },
                                        content: SlackMessageContent { text, .. },
                                        ..
                                    },
                                ..
                            },
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

        Ok(Notification {
            id: Uuid::new_v4().into(),
            title: replace_emoji_code_in_string_with_emoji(&title),
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
