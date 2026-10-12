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

/// Whether `id` is a Notion id: a UUID, with or without dashes. Webhook ids
/// end up in API paths, so nothing else may get through.
pub fn is_notion_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    match bytes.len() {
        32 => bytes.iter().all(u8::is_ascii_hexdigit),
        36 => bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        }),
        _ => false,
    }
}

impl NotionWebhookEvent {
    /// Whether the ids the jobs put in Notion API paths (the page, the
    /// commented block and the updated blocks) are Notion ids. The parent of
    /// a page event (a space, for instance) is never read, so not checked.
    pub fn has_well_formed_resource_ids(&self) -> bool {
        let parent_is_read = self.event_type.is_comment_event();
        self.data.page_id.iter().all(|id| is_notion_id(id))
            && (!parent_is_read
                || self
                    .data
                    .parent
                    .iter()
                    .all(|parent| is_notion_id(&parent.id)))
            && self
                .data
                .updated_blocks
                .iter()
                .all(|block| is_notion_id(&block.id))
    }

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
    #[serde(rename = "page.content_updated")]
    PageContentUpdated,
    #[serde(other)]
    Other,
}

impl NotionWebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CommentCreated => "comment.created",
            Self::CommentUpdated => "comment.updated",
            Self::CommentDeleted => "comment.deleted",
            Self::PageContentUpdated => "page.content_updated",
            Self::Other => "other",
        }
    }

    pub fn is_comment_event(&self) -> bool {
        matches!(
            self,
            Self::CommentCreated | Self::CommentUpdated | Self::CommentDeleted
        )
    }

    /// The events a job handles: comments and page content updates (for
    /// mentions in the page's blocks).
    pub fn is_handled_event(&self) -> bool {
        self.is_comment_event() || *self == Self::PageContentUpdated
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
    /// The blocks changed by a `page.content_updated` event.
    #[serde(default)]
    pub updated_blocks: Vec<NotionWebhookObject>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::*;
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

    #[rstest]
    #[case::dashed("3c612f56-fdd0-4a30-a4d6-bda7d7426309")]
    #[case::undashed("3c612f56fdd04a30a4d6bda7d7426309")]
    #[case::uppercase("3C612F56-FDD0-4A30-A4D6-BDA7D7426309")]
    fn test_notion_ids_are_accepted(#[case] id: &str) {
        assert!(is_notion_id(id));
    }

    #[rstest]
    #[case::path_traversal("../v1/users")]
    #[case::braced("{3c612f56-fdd0-4a30-a4d6-bda7d7426309}")]
    #[case::urn("urn:uuid:3c612f56-fdd0-4a30-a4d6-bda7d7426309")]
    #[case::empty("")]
    #[case::too_short("3c612f56fdd04a30a4d6bda7d742630")]
    #[case::slash("3c612f56-fdd0-4a30-a4d6/bda7d7426309")]
    #[case::query("3c612f56fdd04a30a4d6bda7d742630?")]
    #[case::misplaced_dash("3c612f5-6fdd0-4a30-a4d6-bda7d7426309")]
    fn test_other_ids_are_rejected(#[case] id: &str) {
        assert!(!is_notion_id(id));
    }

    #[rstest]
    #[case::page_id("comment.created", json!({ "page_id": "../v1/users" }))]
    #[case::comment_parent(
        "comment.created",
        json!({ "parent": { "id": "../v1/users", "type": "block" } })
    )]
    #[case::updated_block("page.content_updated", json!({
        "updated_blocks": [
            { "id": "3c612f56-fdd0-4a30-a4d6-bda7d7426309", "type": "block" },
            { "id": "../v1/users", "type": "block" }
        ]
    }))]
    fn test_a_malformed_resource_id_is_detected(
        #[case] event_type: &str,
        #[case] data: serde_json::Value,
    ) {
        assert!(event(event_type, json!({})).has_well_formed_resource_ids());

        assert!(!event(event_type, data).has_well_formed_resource_ids());
    }

    #[test]
    fn test_the_parent_of_a_page_event_is_not_checked() {
        let event = event(
            "page.content_updated",
            json!({ "parent": { "id": "space-id", "type": "space" } }),
        );

        assert!(event.has_well_formed_resource_ids());
    }

    fn event(event_type: &str, data: serde_json::Value) -> NotionWebhookEvent {
        serde_json::from_value(json!({
            "id": "event-id",
            "timestamp": "2026-10-10T09:00:00.000Z",
            "workspace_id": "workspace-id",
            "type": event_type,
            "entity": { "id": "entity-id", "type": "page" },
            "data": data
        }))
        .unwrap()
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
        assert!(!event.event_type.is_handled_event());
        assert_eq!(event.event_type.as_str(), "other");
        assert_eq!(event.accessible_person_ids(), None);
    }

    #[test]
    fn test_parse_page_content_updated_event() {
        let event: NotionWebhookEvent = serde_json::from_value(json!({
            "id": "event-id",
            "timestamp": "2026-10-10T09:00:00.000Z",
            "workspace_id": "workspace-id",
            "type": "page.content_updated",
            "authors": [{ "id": "author-id", "type": "person" }],
            "entity": { "id": "page-id", "type": "page" },
            "data": {
                "parent": { "id": "space-id", "type": "space" },
                "updated_blocks": [
                    { "id": "block-1", "type": "block" },
                    { "id": "block-2", "type": "block" }
                ]
            }
        }))
        .unwrap();

        assert_eq!(event.event_type, NotionWebhookEventType::PageContentUpdated);
        assert_eq!(event.event_type.as_str(), "page.content_updated");
        assert!(event.event_type.is_handled_event());
        assert!(!event.event_type.is_comment_event());
        assert_eq!(
            event
                .data
                .updated_blocks
                .iter()
                .map(|block| block.id.as_str())
                .collect::<Vec<_>>(),
            vec!["block-1", "block-2"]
        );
    }
}
