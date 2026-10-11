use std::{collections::HashMap, time::Duration};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use http::{HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use url::Url;
use uuid::Uuid;
use vec1::Vec1;

use universal_inbox::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{
        Notification, NotificationId, NotificationSource, NotificationSourceKind,
        NotificationStatus,
    },
    pii::Pii,
    third_party::{
        integrations::notion::{
            NotionComment, NotionCommentMessage, NotionCommentParent, NotionPage, NotionUser,
            parse_rich_text,
        },
        item::ThirdPartyItem,
    },
    user::UserId,
};

use crate::{
    integrations::{notification::ThirdPartyNotificationSourceService, oauth2::AccessToken},
    observability::attr,
    universal_inbox::UniversalInboxError,
    utils::api::{ApiClient, ApiClientError},
};

pub mod oauth;
pub mod webhook;

static NOTION_BASE_URL: &str = "https://api.notion.com";
/// The comments, pages, blocks and users endpoints we use are stable in this
/// version.
static NOTION_VERSION: &str = "2022-06-28";

#[derive(Clone)]
pub struct NotionService {
    notion_base_url: String,
    notion_base_path: String,
    max_retry_duration: Duration,
}

/// A Notion user reference, as found in `created_by` or `authors`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct RawNotionUserReference {
    pub id: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawNotionCommentParent {
    PageId { page_id: String },
    BlockId { block_id: String },
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct RawNotionComment {
    pub id: String,
    pub discussion_id: String,
    pub parent: RawNotionCommentParent,
    pub created_by: RawNotionUserReference,
    pub created_time: DateTime<Utc>,
    pub last_edited_time: DateTime<Utc>,
    #[serde(default)]
    pub rich_text: Vec<Value>,
}

#[derive(Deserialize, Debug)]
struct RawNotionCommentList {
    results: Vec<RawNotionComment>,
    #[serde(default)]
    has_more: bool,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct RawNotionPage {
    pub id: String,
    pub url: Url,
    pub created_by: Option<RawNotionUserReference>,
    #[serde(default)]
    pub properties: HashMap<String, Value>,
}

impl RawNotionPage {
    /// The text of the page's `title`-type property.
    pub fn title(&self) -> String {
        self.properties
            .values()
            .find(|property| property["type"] == "title")
            .and_then(|property| property["title"].as_array())
            .map(|segments| {
                segments
                    .iter()
                    .filter_map(|segment| segment["plain_text"].as_str())
                    .collect::<String>()
            })
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Untitled".to_string())
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct RawNotionBlock {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: String,
    pub created_by: Option<RawNotionUserReference>,
    #[serde(flatten)]
    pub content: HashMap<String, Value>,
}

impl RawNotionBlock {
    /// The block's rich text segments. A `table_row`'s cells are flattened,
    /// separated by a ` | ` text segment.
    pub fn rich_text(&self) -> Vec<Value> {
        let content = self.content.get(&self.block_type);
        if self.block_type == "table_row" {
            let cells = content
                .and_then(|content| content["cells"].as_array())
                .cloned()
                .unwrap_or_default();
            let mut segments = vec![];
            for (index, cell) in cells.iter().enumerate() {
                if index > 0 {
                    segments.push(text_segment(" | "));
                }
                segments.extend(cell.as_array().cloned().unwrap_or_default());
            }
            return segments;
        }
        content
            .and_then(|content| content["rich_text"].as_array())
            .cloned()
            .unwrap_or_default()
    }
}

fn text_segment(content: &str) -> Value {
    serde_json::json!({
        "type": "text",
        "text": { "content": content, "link": null },
        "annotations": {
            "bold": false, "italic": false, "strikethrough": false,
            "underline": false, "code": false, "color": "default"
        },
        "plain_text": content,
        "href": null
    })
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
struct RawNotionUser {
    id: String,
    name: Option<String>,
    avatar_url: Option<Url>,
}

/// User ids mentioned in a raw Notion `rich_text` array.
pub fn mentioned_user_ids_in_raw_rich_text(rich_text: &[Value]) -> Vec<String> {
    rich_text
        .iter()
        .filter(|segment| segment["type"] == "mention" && segment["mention"]["type"] == "user")
        .filter_map(|segment| segment["mention"]["user"]["id"].as_str())
        .map(|id| id.to_string())
        .collect()
}

/// Whether a Notion call failed because the integration cannot see the
/// object (the page is not shared with it).
pub fn is_access_denied(error: &UniversalInboxError) -> bool {
    matches!(error, UniversalInboxError::Forbidden(_))
}

/// GET a Notion object. Notion answers 404 (or 403) for an object the
/// integration cannot see: that becomes `Forbidden`.
async fn notion_get<R: serde::de::DeserializeOwned>(
    client: &ApiClient,
    url: String,
    object: &str,
) -> Result<R, UniversalInboxError> {
    match client.get(url).await {
        Ok(response) => Ok(response),
        Err(ApiClientError::NetworkError(error))
            if error
                .status()
                .is_some_and(|status| status.as_u16() == 404 || status.as_u16() == 403) =>
        {
            Err(UniversalInboxError::Forbidden(format!(
                "Notion {object} is not accessible to the integration (HTTP {})",
                error
                    .status()
                    .map(|status| status.as_u16())
                    .unwrap_or_default()
            )))
        }
        Err(error) => Err(anyhow!(error)
            .context(format!("Failed to fetch Notion {object}"))
            .into()),
    }
}

impl NotionService {
    pub fn new(
        notion_base_url: Option<String>,
        max_retry_duration: Duration,
    ) -> Result<NotionService, UniversalInboxError> {
        let notion_base_url = notion_base_url.unwrap_or_else(|| NOTION_BASE_URL.to_string());
        let notion_base_path = Url::parse(&notion_base_url)
            .context("Failed to parse Notion base URL")?
            .path()
            .trim_end_matches('/')
            .to_string();
        Ok(NotionService {
            notion_base_url: notion_base_url.trim_end_matches('/').to_string(),
            notion_base_path,
            max_retry_duration,
        })
    }

    fn build_notion_client(
        &self,
        access_token: &AccessToken,
    ) -> Result<ApiClient, UniversalInboxError> {
        let mut headers = HeaderMap::new();
        let mut auth_header_value: HeaderValue =
            format!("Bearer {}", access_token.as_str()).parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);
        headers.insert("Notion-Version", HeaderValue::from_static(NOTION_VERSION));

        ApiClient::build(
            headers,
            otel_known_paths(&self.notion_base_path),
            self.max_retry_duration,
        )
    }

    /// Every comment attached to `block_id` (a page or a block), oldest
    /// first.
    pub async fn list_comments(
        &self,
        access_token: &AccessToken,
        block_id: &str,
    ) -> Result<Vec<RawNotionComment>, UniversalInboxError> {
        let client = self.build_notion_client(access_token)?;
        let mut comments = vec![];
        let mut start_cursor: Option<String> = None;
        loop {
            let url = format!(
                "{}/v1/comments?block_id={block_id}&page_size=100{}",
                self.notion_base_url,
                start_cursor
                    .map(|cursor| format!("&start_cursor={cursor}"))
                    .unwrap_or_default()
            );
            let page: RawNotionCommentList = notion_get(&client, url, "comments").await?;
            comments.extend(page.results);
            if !page.has_more || page.next_cursor.is_none() {
                break;
            }
            start_cursor = page.next_cursor;
        }
        comments.sort_by_key(|comment| comment.created_time);
        Ok(comments)
    }

    pub async fn get_page(
        &self,
        access_token: &AccessToken,
        page_id: &str,
    ) -> Result<RawNotionPage, UniversalInboxError> {
        notion_get(
            &self.build_notion_client(access_token)?,
            format!("{}/v1/pages/{page_id}", self.notion_base_url),
            "page",
        )
        .await
    }

    pub async fn get_block(
        &self,
        access_token: &AccessToken,
        block_id: &str,
    ) -> Result<RawNotionBlock, UniversalInboxError> {
        notion_get(
            &self.build_notion_client(access_token)?,
            format!("{}/v1/blocks/{block_id}", self.notion_base_url),
            "block",
        )
        .await
    }

    /// The user's name and avatar. Without the user information capability
    /// Notion refuses the call: the user is then known by id only.
    async fn get_user(&self, access_token: &AccessToken, user_id: &str) -> NotionUser {
        let user: Result<RawNotionUser, UniversalInboxError> = async {
            notion_get(
                &self.build_notion_client(access_token)?,
                format!("{}/v1/users/{user_id}", self.notion_base_url),
                "user",
            )
            .await
        }
        .await;
        match user {
            Ok(user) => NotionUser {
                id: user.id,
                name: user.name.map(Pii::new),
                avatar_url: user.avatar_url,
            },
            Err(error) => {
                tracing::debug!("Unable to fetch Notion user: {error:?}");
                NotionUser {
                    id: user_id.to_string(),
                    name: None,
                    avatar_url: None,
                }
            }
        }
    }

    /// Read a discussion with the user's token. `None` when every comment of
    /// the discussion was deleted.
    pub async fn fetch_discussion(
        &self,
        access_token: &AccessToken,
        page_id: &str,
        parent_id: &str,
        discussion_id: &str,
        user_notion_id: &str,
    ) -> Result<Option<NotionComment>, UniversalInboxError> {
        let raw_comments: Vec<RawNotionComment> = self
            .list_comments(access_token, parent_id)
            .await?
            .into_iter()
            .filter(|comment| comment.discussion_id == discussion_id)
            .collect();
        let Some(first_comment) = raw_comments.first() else {
            return Ok(None);
        };
        let parent = match &first_comment.parent {
            RawNotionCommentParent::PageId { page_id } => NotionCommentParent::Page {
                page_id: page_id.clone(),
            },
            RawNotionCommentParent::BlockId { block_id } => NotionCommentParent::Block {
                block_id: block_id.clone(),
            },
        };

        let page = self.get_page(access_token, page_id).await?;
        let (quoted_content, parent_block_created_by) = match &parent {
            NotionCommentParent::Block { block_id } => {
                let block = self.get_block(access_token, block_id).await?;
                (
                    Some(parse_rich_text(&block.rich_text())),
                    block.created_by.map(|user| user.id),
                )
            }
            NotionCommentParent::Page { .. } => (None, None),
        };

        let mut users: HashMap<String, NotionUser> = HashMap::new();
        let mut comments = vec![];
        for raw_comment in raw_comments {
            let author_id = raw_comment.created_by.id.clone();
            if !users.contains_key(&author_id) {
                let user = self.get_user(access_token, &author_id).await;
                users.insert(author_id.clone(), user);
            }
            comments.push(NotionCommentMessage {
                id: raw_comment.id,
                author: users[&author_id].clone(),
                rich_text: parse_rich_text(&raw_comment.rich_text),
                created_time: raw_comment.created_time,
                last_edited_time: raw_comment.last_edited_time,
            });
        }

        Ok(Some(NotionComment {
            discussion_id: discussion_id.to_string(),
            page: NotionPage {
                title: page.title(),
                id: page.id,
                url: page.url,
                created_by: page.created_by.map(|user| user.id),
            },
            parent,
            quoted_content,
            parent_block_created_by,
            comments: Vec1::try_from_vec(comments)
                .map_err(|_| anyhow!("A Notion discussion must have comments"))?,
            user_notion_id: Some(user_notion_id.to_string()),
        }))
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<NotionComment> for NotionService {
    async fn third_party_item_into_notification(
        &self,
        source: &NotionComment,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let latest_comment = source.latest_comment();
        let author = latest_comment
            .author
            .name
            .as_ref()
            .map(|name| name.expose().clone())
            .unwrap_or_else(|| "someone".to_string());
        // The user wrote the last comment: nothing new for them.
        let status = if source
            .user_notion_id
            .as_deref()
            .is_some_and(|user_notion_id| source.is_last_comment_from_user(user_notion_id))
        {
            NotificationStatus::Deleted
        } else {
            NotificationStatus::Unread
        };

        Ok(Box::new(Notification {
            id: NotificationId(Uuid::new_v4()),
            title: format!("Comment by {author} on {}", source.page.title),
            status,
            created_at: source.comments.first().created_time,
            updated_at: latest_comment.last_edited_time,
            last_read_at: None,
            snoozed_until: None,
            user_id,
            task_id: None,
            kind: NotificationSourceKind::Notion,
            source_item: source_third_party_item.clone(),
        }))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::THIRD_PARTY_ITEM_SOURCE_ID } = source_item.source_id)
    )]
    async fn delete_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // The Notion public API cannot archive or mark a comment as read
        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::THIRD_PARTY_ITEM_SOURCE_ID } = source_item.source_id)
    )]
    async fn unsubscribe_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // The Notion public API cannot unfollow a discussion
        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::THIRD_PARTY_ITEM_SOURCE_ID } = source_item.source_id)
    )]
    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Notion has no snooze
        Ok(())
    }
}

impl NotificationSource for NotionService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Notion
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
    }
}

impl IntegrationProviderSource for NotionService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Notion
    }
}

/// Route templates used to name outbound request spans. Every path the client
/// calls must match one of them, otherwise the span is named `<METHOD> UNKNOWN`.
fn otel_known_paths(base_path: &str) -> Vec<String> {
    [
        "/v1/comments",
        "/v1/pages/{page_id}",
        "/v1/blocks/{block_id}",
        "/v1/users/{user_id}",
    ]
    .iter()
    .map(|path| format!("{base_path}{path}"))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn test_page_title_is_read_from_the_title_property() {
        let page: RawNotionPage = serde_json::from_value(json!({
            "id": "page-id",
            "url": "https://www.notion.so/Roadmap-pageid",
            "created_by": { "object": "user", "id": "creator-id" },
            "properties": {
                "Status": { "type": "select", "select": null },
                "Name": {
                    "type": "title",
                    "title": [
                        { "type": "text", "plain_text": "Road" },
                        { "type": "text", "plain_text": "map" }
                    ]
                }
            }
        }))
        .unwrap();

        assert_eq!(page.title(), "Roadmap");
        assert_eq!(page.created_by.unwrap().id, "creator-id");
    }

    #[test]
    fn test_page_without_title_is_untitled() {
        let page: RawNotionPage = serde_json::from_value(json!({
            "id": "page-id",
            "url": "https://www.notion.so/pageid",
            "properties": {}
        }))
        .unwrap();

        assert_eq!(page.title(), "Untitled");
    }

    #[test]
    fn test_paragraph_block_rich_text() {
        let block: RawNotionBlock = serde_json::from_value(json!({
            "id": "block-id",
            "type": "paragraph",
            "created_by": { "object": "user", "id": "author-id" },
            "paragraph": { "rich_text": [{ "type": "text", "plain_text": "Hello" }] }
        }))
        .unwrap();

        assert_eq!(
            block.rich_text(),
            vec![json!({ "type": "text", "plain_text": "Hello" })]
        );
    }

    #[test]
    fn test_table_row_cells_are_joined() {
        let block: RawNotionBlock = serde_json::from_value(json!({
            "id": "block-id",
            "type": "table_row",
            "table_row": {
                "cells": [
                    [{ "type": "text", "plain_text": "A" }],
                    [{ "type": "text", "plain_text": "B" }]
                ]
            }
        }))
        .unwrap();

        let plain_text: Vec<String> = block
            .rich_text()
            .iter()
            .map(|segment| segment["plain_text"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(plain_text, vec!["A", " | ", "B"]);
    }

    #[test]
    fn test_mentioned_user_ids_in_raw_rich_text() {
        let rich_text = vec![
            json!({ "type": "text", "plain_text": "Hi " }),
            json!({ "type": "mention", "mention": { "type": "user", "user": { "id": "user-1" } } }),
            json!({ "type": "mention", "mention": { "type": "page", "page": { "id": "page-1" } } }),
        ];

        assert_eq!(
            mentioned_user_ids_in_raw_rich_text(&rich_text),
            vec!["user-1".to_string()]
        );
    }
}
