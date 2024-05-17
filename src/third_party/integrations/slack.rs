use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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

    pub fn title(&self) -> String {
        match self {
            SlackStarredItem::SlackMessage(message) => message
                .message
                .content
                .text
                .clone()
                .unwrap_or("Starred message".to_string()),
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
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
