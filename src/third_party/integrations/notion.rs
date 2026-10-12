use anyhow::anyhow;
use chrono::{DateTime, Utc};
use notionrs_types::object::rich_text::{RichText, mention::Mention, text::Text};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
use uuid::Uuid;
use vec1::Vec1;

use crate::{
    HasHtmlUrl,
    integration_connection::IntegrationConnectionId,
    pii::Pii,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    typed_id::TypedId,
    user::UserId,
};

/// A Notion user. Comment authors and mentions only carry the `id`; the
/// name is resolved separately (`GET /v1/users/{id}`).
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct NotionUser {
    pub id: String,
    pub name: Option<Pii<String>>,
    pub avatar_url: Option<Url>,
}

/// The page a comment or a mentioning block belongs to.
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct NotionPage {
    pub id: String,
    pub title: String,
    /// The page object's `url`.
    pub url: Url,
    /// Notion user id of the page's creator (`created_by.id`).
    #[serde(default)]
    pub created_by: Option<String>,
}

/// Where a discussion is attached: the page itself, or a block of it (block
/// comments and comments on selected text).
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
#[serde(tag = "type", content = "content")]
pub enum NotionCommentParent {
    Page { page_id: String },
    Block { block_id: String },
}

/// A Notion discussion thread. One item per `discussion_id`.
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct NotionComment {
    pub discussion_id: String,
    pub page: NotionPage,
    pub parent: NotionCommentParent,
    /// The commented block's text, shown above the discussion. `None` for a
    /// page-level discussion. The public API does not expose the selected
    /// range, so this is the whole block.
    #[serde(default)]
    pub quoted_content: Option<Vec<RichText>>,
    /// Notion user id of the commented block's creator, when the discussion
    /// is attached to a block.
    #[serde(default)]
    pub parent_block_created_by: Option<String>,
    /// Oldest first: the first comment opens the thread, the others are
    /// replies.
    pub comments: Vec1<NotionCommentMessage>,
    /// Notion user id of the Universal Inbox user this item belongs to.
    #[serde(default)]
    pub user_notion_id: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct NotionCommentMessage {
    pub id: String,
    pub author: NotionUser,
    pub rich_text: Vec<RichText>,
    pub created_time: DateTime<Utc>,
    pub last_edited_time: DateTime<Utc>,
}

/// A page block mentioning the user of the integration connection. One item
/// per block.
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct NotionBlock {
    pub id: String,
    /// Notion block type, e.g. `paragraph` or `table_row`.
    pub block_type: String,
    pub page: NotionPage,
    /// The block's text. A `table_row`'s cells are flattened, with a ` | `
    /// text segment between cells.
    pub rich_text: Vec<RichText>,
    pub last_edited_by: Option<NotionUser>,
    pub last_edited_time: DateTime<Utc>,
    /// Notion user id of the Universal Inbox user this item belongs to.
    #[serde(default)]
    pub user_notion_id: Option<String>,
}

/// Parse a Notion `rich_text` array. A segment `notionrs_types` cannot read
/// (a new or unsupported mention type) is kept as plain text instead of
/// failing the whole array.
pub fn parse_rich_text(segments: &[Value]) -> Vec<RichText> {
    segments
        .iter()
        .map(|segment| {
            serde_json::from_value::<RichText>(segment.clone()).unwrap_or_else(|_| {
                let plain_text = segment["plain_text"].as_str().unwrap_or_default();
                RichText::Text {
                    text: Text::from(plain_text),
                    annotations: serde_json::from_value(segment["annotations"].clone())
                        .unwrap_or_default(),
                    plain_text: plain_text.to_string(),
                    href: segment["href"].as_str().map(|href| href.to_string()),
                }
            })
        })
        .collect()
}

pub fn to_plain_text(rich_text: &[RichText]) -> String {
    rich_text
        .iter()
        .map(|segment| match segment {
            RichText::Text { plain_text, .. }
            | RichText::Mention { plain_text, .. }
            | RichText::Equation { plain_text, .. } => plain_text.as_str(),
        })
        .collect()
}

pub fn mentioned_user_ids(rich_text: &[RichText]) -> Vec<String> {
    rich_text
        .iter()
        .filter_map(|segment| match segment {
            RichText::Mention {
                mention: Mention::User { user },
                ..
            } => Some(user.id.clone()),
            _ => None,
        })
        .collect()
}

fn mentions_user(rich_text: &[RichText], notion_user_id: &str) -> bool {
    mentioned_user_ids(rich_text)
        .iter()
        .any(|id| id == notion_user_id)
}

impl NotionCommentMessage {
    pub fn mentions_user(&self, notion_user_id: &str) -> bool {
        mentions_user(&self.rich_text, notion_user_id)
    }
}

impl NotionBlock {
    pub fn mentions_user(&self, notion_user_id: &str) -> bool {
        mentions_user(&self.rich_text, notion_user_id)
    }

    /// The block no longer mentions the user it was saved for.
    pub fn is_mention_removed(&self) -> bool {
        self.user_notion_id
            .as_deref()
            .is_some_and(|user_notion_id| !self.mentions_user(user_notion_id))
    }
}

impl NotionComment {
    pub fn latest_comment(&self) -> &NotionCommentMessage {
        self.comments.last()
    }

    /// The user created the page or the commented block, wrote in the
    /// discussion, or is mentioned in it.
    pub fn is_user_involved(&self, notion_user_id: &str) -> bool {
        self.page.created_by.as_deref() == Some(notion_user_id)
            || self.parent_block_created_by.as_deref() == Some(notion_user_id)
            || self.comments.iter().any(|comment| {
                comment.author.id == notion_user_id || comment.mentions_user(notion_user_id)
            })
    }

    pub fn is_last_comment_from_user(&self, notion_user_id: &str) -> bool {
        self.latest_comment().author.id == notion_user_id
    }

    /// A comment written or edited after `since` mentions the user: it
    /// brings an unsubscribed discussion back.
    pub fn mentions_user_since(&self, notion_user_id: &str, since: DateTime<Utc>) -> bool {
        self.comments.iter().any(|comment| {
            comment.last_edited_time > since && comment.mentions_user(notion_user_id)
        })
    }

    /// Split the replies (all comments but the first) into those already
    /// seen, last edited before `seen_before`, and the newer ones.
    pub fn split_seen_replies(
        &self,
        seen_before: DateTime<Utc>,
    ) -> (&[NotionCommentMessage], &[NotionCommentMessage]) {
        let replies = &self.comments[1..];
        let first_unseen_reply_index = replies
            .iter()
            .position(|reply| reply.last_edited_time >= seen_before)
            .unwrap_or(replies.len());
        replies.split_at(first_unseen_reply_index)
    }
}

fn notion_id_without_dashes(id: &str) -> String {
    id.replace('-', "")
}

impl HasHtmlUrl for NotionComment {
    fn get_html_url(&self) -> Url {
        let mut url = self.page.url.clone();
        url.set_fragment(None);
        url.query_pairs_mut()
            .clear()
            .append_pair("d", &notion_id_without_dashes(&self.discussion_id));
        url
    }
}

impl HasHtmlUrl for NotionBlock {
    fn get_html_url(&self) -> Url {
        let mut url = self.page.url.clone();
        url.set_fragment(Some(&notion_id_without_dashes(&self.id)));
        url
    }
}

impl ThirdPartyItemFromSource for NotionComment {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: TypedId::new(Uuid::new_v4()),
            source_id: self.source_id(),
            data: ThirdPartyItemData::NotionComment(Box::new(self.clone())),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }

    fn source_id(&self) -> String {
        self.discussion_id.clone()
    }
}

impl ThirdPartyItemFromSource for NotionBlock {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: TypedId::new(Uuid::new_v4()),
            source_id: self.source_id(),
            data: ThirdPartyItemData::NotionBlock(Box::new(self.clone())),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }

    fn source_id(&self) -> String {
        self.id.clone()
    }
}

impl TryFrom<ThirdPartyItem> for NotionComment {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::NotionComment(comment) => Ok(*comment),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to NotionComment",
                item.id
            )),
        }
    }
}

impl TryFrom<ThirdPartyItem> for NotionBlock {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::NotionBlock(block) => Ok(*block),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to NotionBlock",
                item.id
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use rstest::*;
    use serde_json::json;

    const USER_ID: &str = "9188c6a5-7381-452f-b3dc-d4865aa89bdf";
    const OTHER_USER_ID: &str = "e79a0b74-3aba-4149-9f74-0bb5791a6ee6";
    const PAGE_CREATOR_ID: &str = "0f1e2d3c-0000-4000-8000-0000000000cc";
    const BLOCK_CREATOR_ID: &str = "0f1e2d3c-0000-4000-8000-0000000000dd";

    fn annotations() -> Value {
        json!({
            "bold": false, "italic": false, "strikethrough": false,
            "underline": false, "code": false, "color": "default"
        })
    }

    fn text(content: &str) -> Value {
        json!({
            "type": "text",
            "text": { "content": content, "link": null },
            "annotations": annotations(),
            "plain_text": content,
            "href": null
        })
    }

    fn user_mention(id: &str, name: &str) -> Value {
        json!({
            "type": "mention",
            "mention": { "type": "user", "user": { "object": "user", "id": id } },
            "annotations": annotations(),
            "plain_text": format!("@{name}"),
            "href": null
        })
    }

    fn rich_text(segments: Vec<Value>) -> Vec<RichText> {
        parse_rich_text(&segments)
    }

    fn notion_user(id: &str, name: &str) -> NotionUser {
        NotionUser {
            id: id.to_string(),
            name: Some(Pii::new(name.to_string())),
            avatar_url: None,
        }
    }

    #[fixture]
    fn notion_page() -> NotionPage {
        NotionPage {
            id: "3c612f56-fdd0-4a30-a4d6-bda7d7426309".to_string(),
            title: "Roadmap".to_string(),
            url: "https://www.notion.so/Roadmap-3c612f56fdd04a30a4d6bda7d7426309"
                .parse()
                .unwrap(),
            created_by: Some(PAGE_CREATOR_ID.to_string()),
        }
    }

    fn comment_message(
        id: &str,
        author_id: &str,
        minute: u32,
        text: Vec<Value>,
    ) -> NotionCommentMessage {
        let time = Utc.with_ymd_and_hms(2026, 10, 10, 9, minute, 0).unwrap();
        NotionCommentMessage {
            id: id.to_string(),
            author: notion_user(author_id, "Jane Doe"),
            rich_text: rich_text(text),
            created_time: time,
            last_edited_time: time,
        }
    }

    #[fixture]
    fn notion_comment(notion_page: NotionPage) -> NotionComment {
        NotionComment {
            discussion_id: "a2b6c3d4-0000-4000-8000-0000000000aa".to_string(),
            page: notion_page,
            parent: NotionCommentParent::Block {
                block_id: "b1b2b3b4-0000-4000-8000-0000000000bb".to_string(),
            },
            quoted_content: Some(rich_text(vec![text("The quoted paragraph")])),
            parent_block_created_by: Some(BLOCK_CREATOR_ID.to_string()),
            user_notion_id: Some(USER_ID.to_string()),
            comments: Vec1::try_from_vec(vec![
                comment_message(
                    "c1",
                    OTHER_USER_ID,
                    0,
                    vec![
                        text("What do you think "),
                        user_mention(USER_ID, "John Doe"),
                        text("?"),
                    ],
                ),
                comment_message("c2", OTHER_USER_ID, 10, vec![text("Any news?")]),
                comment_message("c3", OTHER_USER_ID, 20, vec![text("Ping")]),
            ])
            .unwrap(),
        }
    }

    #[fixture]
    fn notion_block(notion_page: NotionPage) -> NotionBlock {
        NotionBlock {
            id: "b1b2b3b4-0000-4000-8000-0000000000bb".to_string(),
            block_type: "paragraph".to_string(),
            page: notion_page,
            rich_text: rich_text(vec![
                user_mention(USER_ID, "John Doe"),
                text(" please review"),
            ]),
            last_edited_by: Some(notion_user(OTHER_USER_ID, "Jane Doe")),
            last_edited_time: Utc.with_ymd_and_hms(2026, 10, 10, 9, 0, 0).unwrap(),
            user_notion_id: Some(USER_ID.to_string()),
        }
    }

    #[test]
    fn test_rich_text_round_trips_notion_json() {
        let segments = vec![
            json!({
                "type": "text",
                "text": { "content": "see ", "link": { "url": "https://example.com" } },
                "annotations": {
                    "bold": true, "italic": false, "strikethrough": false,
                    "underline": false, "code": false, "color": "red_background"
                },
                "plain_text": "see ",
                "href": "https://example.com"
            }),
            user_mention(USER_ID, "John Doe"),
            json!({
                "type": "mention",
                "mention": { "type": "page", "page": { "id": "3c612f56-fdd0-4a30-a4d6-bda7d7426309" } },
                "annotations": annotations(),
                "plain_text": "Roadmap",
                "href": "https://www.notion.so/3c612f56fdd04a30a4d6bda7d7426309"
            }),
            json!({
                "type": "mention",
                "mention": { "type": "date", "date": { "start": "2026-10-10", "end": null, "time_zone": null } },
                "annotations": annotations(),
                "plain_text": "2026-10-10",
                "href": null
            }),
            json!({
                "type": "equation",
                "equation": { "expression": "E = mc^2" },
                "annotations": annotations(),
                "plain_text": "E = mc^2",
                "href": null
            }),
        ];

        let parsed = parse_rich_text(&segments);

        assert_eq!(serde_json::to_value(&parsed).unwrap(), json!(segments));
    }

    #[test]
    fn test_unparsable_rich_text_segment_is_kept_as_plain_text() {
        // notionrs_types 0.24 fails on template mentions.
        let segments = vec![json!({
            "type": "mention",
            "mention": {
                "type": "template_mention",
                "template_mention": { "type": "template_mention_user", "template_mention_user": "me" }
            },
            "annotations": annotations(),
            "plain_text": "@Me",
            "href": null
        })];

        let parsed = parse_rich_text(&segments);

        assert_eq!(parsed, rich_text(vec![text("@Me")]));
    }

    #[rstest]
    fn test_rich_text_helpers(notion_comment: NotionComment) {
        let first_comment = notion_comment.comments.first();

        assert_eq!(
            to_plain_text(&first_comment.rich_text),
            "What do you think @John Doe?"
        );
        assert_eq!(
            mentioned_user_ids(&first_comment.rich_text),
            vec![USER_ID.to_string()]
        );
        assert!(first_comment.mentions_user(USER_ID));
        assert!(!first_comment.mentions_user(OTHER_USER_ID));
    }

    #[rstest]
    fn test_personal_data_is_redacted_in_debug_and_kept_on_the_wire(notion_comment: NotionComment) {
        let debug = format!("{notion_comment:?}");
        assert!(!debug.contains("Jane Doe"));

        let wire = serde_json::to_value(&notion_comment).unwrap();
        assert_eq!(wire["comments"][0]["author"]["name"], "Jane Doe");
        assert_eq!(
            serde_json::from_value::<NotionComment>(wire).unwrap(),
            notion_comment
        );
    }

    #[rstest]
    fn test_third_party_item_data_round_trips(
        notion_comment: NotionComment,
        notion_block: NotionBlock,
    ) {
        for (data, tag) in [
            (
                ThirdPartyItemData::NotionComment(Box::new(notion_comment)),
                "NotionComment",
            ),
            (
                ThirdPartyItemData::NotionBlock(Box::new(notion_block)),
                "NotionBlock",
            ),
        ] {
            let wire = serde_json::to_value(&data).unwrap();
            assert_eq!(wire["type"], tag);
            assert_eq!(
                serde_json::from_value::<ThirdPartyItemData>(wire).unwrap(),
                data
            );
        }
    }

    #[rstest]
    fn test_missing_quoted_content_is_none(notion_comment: NotionComment) {
        let mut wire = serde_json::to_value(&notion_comment).unwrap();
        wire.as_object_mut().unwrap().remove("quoted_content");

        let comment: NotionComment = serde_json::from_value(wire).unwrap();

        assert_eq!(comment.quoted_content, None);
    }

    #[rstest]
    fn test_notion_comment_html_url(notion_comment: NotionComment) {
        assert_eq!(
            notion_comment.get_html_url().as_str(),
            "https://www.notion.so/Roadmap-3c612f56fdd04a30a4d6bda7d7426309?d=a2b6c3d40000400080000000000000aa"
        );
    }

    #[rstest]
    fn test_notion_block_html_url(notion_block: NotionBlock) {
        assert_eq!(
            notion_block.get_html_url().as_str(),
            "https://www.notion.so/Roadmap-3c612f56fdd04a30a4d6bda7d7426309#b1b2b3b40000400080000000000000bb"
        );
    }

    #[rstest]
    fn test_notion_comment_third_party_item_creation(notion_comment: NotionComment) {
        let user_id = UserId(Uuid::new_v4());
        let integration_connection_id = IntegrationConnectionId(Uuid::new_v4());

        let item = notion_comment
            .clone()
            .into_third_party_item(user_id, integration_connection_id);

        assert_eq!(item.source_id, notion_comment.discussion_id);
        assert_eq!(item.user_id, user_id);
        assert_eq!(item.integration_connection_id, integration_connection_id);
        assert_eq!(item.marked_as_done(), item);
        assert_eq!(NotionComment::try_from(item).unwrap(), notion_comment);
    }

    #[rstest]
    fn test_notion_block_mention_removed(notion_block: NotionBlock) {
        assert!(!notion_block.is_mention_removed());

        let without_mention = NotionBlock {
            rich_text: rich_text(vec![text("Never mind")]),
            ..notion_block.clone()
        };
        assert!(without_mention.is_mention_removed());

        let without_user = NotionBlock {
            user_notion_id: None,
            ..without_mention
        };
        assert!(!without_user.is_mention_removed());
    }

    #[rstest]
    fn test_notion_block_without_user_notion_id_deserializes(notion_block: NotionBlock) {
        let mut value = serde_json::to_value(&notion_block).unwrap();
        value.as_object_mut().unwrap().remove("user_notion_id");

        let block: NotionBlock = serde_json::from_value(value).unwrap();

        assert_eq!(block.user_notion_id, None);
    }

    #[rstest]
    fn test_notion_block_third_party_item_creation(notion_block: NotionBlock) {
        let user_id = UserId(Uuid::new_v4());
        let integration_connection_id = IntegrationConnectionId(Uuid::new_v4());

        let item = notion_block
            .clone()
            .into_third_party_item(user_id, integration_connection_id);

        assert_eq!(item.source_id, notion_block.id);
        assert_eq!(item.marked_as_done(), item);
        assert!(notion_block.mentions_user(USER_ID));
        assert_eq!(NotionBlock::try_from(item).unwrap(), notion_block);
    }

    #[rstest]
    #[case::mentioned(USER_ID, true)]
    #[case::author(OTHER_USER_ID, true)]
    #[case::page_creator(PAGE_CREATOR_ID, true)]
    #[case::block_creator(BLOCK_CREATOR_ID, true)]
    #[case::neither("someone-else", false)]
    fn test_is_user_involved(
        notion_comment: NotionComment,
        #[case] notion_user_id: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(notion_comment.is_user_involved(notion_user_id), expected);
    }

    #[rstest]
    fn test_is_last_comment_from_user(mut notion_comment: NotionComment) {
        assert!(!notion_comment.is_last_comment_from_user(USER_ID));

        notion_comment
            .comments
            .push(comment_message("c4", USER_ID, 30, vec![text("Done")]));

        assert!(notion_comment.is_last_comment_from_user(USER_ID));
        assert_eq!(notion_comment.latest_comment().id, "c4");
    }

    // The fixture mentions USER_ID once, in its first comment at 09:00
    #[rstest]
    #[case::mention_after_since(USER_ID, (8, 55), true)]
    #[case::mention_before_since(USER_ID, (9, 5), false)]
    #[case::other_user_not_mentioned(OTHER_USER_ID, (8, 55), false)]
    fn test_mentions_user_since(
        notion_comment: NotionComment,
        #[case] notion_user_id: &str,
        #[case] (since_hour, since_minute): (u32, u32),
        #[case] expected: bool,
    ) {
        let since = Utc
            .with_ymd_and_hms(2026, 10, 10, since_hour, since_minute, 0)
            .unwrap();

        assert_eq!(
            notion_comment.mentions_user_since(notion_user_id, since),
            expected
        );
    }

    #[rstest]
    #[case::none_seen(0, 0, 2)]
    #[case::some_seen(15, 1, 1)]
    #[case::all_seen(30, 2, 0)]
    fn test_split_seen_replies(
        notion_comment: NotionComment,
        #[case] seen_before_minute: u32,
        #[case] expected_seen: usize,
        #[case] expected_unseen: usize,
    ) {
        let seen_before = Utc
            .with_ymd_and_hms(2026, 10, 10, 9, seen_before_minute, 0)
            .unwrap();

        let (seen, unseen) = notion_comment.split_seen_replies(seen_before);

        assert_eq!((seen.len(), unseen.len()), (expected_seen, expected_unseen));
    }

    #[rstest]
    fn test_split_seen_replies_of_a_single_comment_thread(mut notion_comment: NotionComment) {
        notion_comment.comments.truncate(1).unwrap();

        let (seen, unseen) = notion_comment.split_seen_replies(Utc::now());

        assert!(seen.is_empty() && unseen.is_empty());
    }
}
