use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A Notion webhook event. Notion only sends ids: the content must be read
/// back from the API.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct NotionWebhookEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub workspace_id: String,
    #[serde(rename = "type")]
    pub event_type: NotionWebhookEventType,
    pub entity: NotionWebhookObject,
    #[serde(default)]
    pub authors: Vec<NotionWebhookObject>,
    /// The users and bots of the integration who can see the entity. Only
    /// sent to public integrations.
    #[serde(default)]
    pub accessible_by: Vec<NotionWebhookObject>,
    #[serde(default)]
    pub data: NotionWebhookEventData,
}

impl NotionWebhookEvent {
    /// Person ids in `accessible_by`; `None` when Notion did not send the
    /// list.
    pub fn accessible_person_ids(&self) -> Option<Vec<&str>> {
        if self.accessible_by.is_empty() {
            return None;
        }
        Some(
            self.accessible_by
                .iter()
                .filter(|object| object.object_type == "person")
                .map(|object| object.id.as_str())
                .collect(),
        )
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotionWebhookEventType {
    #[serde(rename = "comment.created")]
    CommentCreated,
    #[serde(rename = "comment.updated")]
    CommentUpdated,
    #[serde(rename = "comment.deleted")]
    CommentDeleted,
    #[serde(other)]
    Other,
}

impl NotionWebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CommentCreated => "comment.created",
            Self::CommentUpdated => "comment.updated",
            Self::CommentDeleted => "comment.deleted",
            Self::Other => "other",
        }
    }

    pub fn is_comment_event(&self) -> bool {
        matches!(
            self,
            Self::CommentCreated | Self::CommentUpdated | Self::CommentDeleted
        )
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct NotionWebhookObject {
    pub id: String,
    #[serde(rename = "type")]
    pub object_type: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Default)]
pub struct NotionWebhookEventData {
    pub page_id: Option<String>,
    pub parent: Option<NotionWebhookObject>,
    pub discussion_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn test_parse_comment_created_event() {
        let event: NotionWebhookEvent = serde_json::from_value(json!({
            "id": "event-id",
            "timestamp": "2026-10-10T09:00:00.000Z",
            "workspace_id": "workspace-id",
            "workspace_name": "Acme",
            "subscription_id": "subscription-id",
            "integration_id": "integration-id",
            "type": "comment.created",
            "authors": [{ "id": "author-id", "type": "person" }],
            "accessible_by": [
                { "id": "person-id", "type": "person" },
                { "id": "bot-id", "type": "bot" }
            ],
            "attempt_number": 1,
            "entity": { "id": "comment-id", "type": "comment" },
            "data": {
                "page_id": "page-id",
                "parent": { "id": "block-id", "type": "block" },
                "discussion_id": "discussion-id"
            }
        }))
        .unwrap();

        assert_eq!(event.event_type, NotionWebhookEventType::CommentCreated);
        assert_eq!(event.data.discussion_id.as_deref(), Some("discussion-id"));
        assert_eq!(event.accessible_person_ids(), Some(vec!["person-id"]));
    }

    #[test]
    fn test_unknown_event_type_is_other() {
        let event: NotionWebhookEvent = serde_json::from_value(json!({
            "id": "event-id",
            "timestamp": "2026-10-10T09:00:00.000Z",
            "workspace_id": "workspace-id",
            "type": "page.locked",
            "entity": { "id": "page-id", "type": "page" },
            "data": {}
        }))
        .unwrap();

        assert_eq!(event.event_type, NotionWebhookEventType::Other);
        assert!(!event.event_type.is_comment_event());
        assert_eq!(event.accessible_person_ids(), None);
    }
}
