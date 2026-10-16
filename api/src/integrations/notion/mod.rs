use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use http::{HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::debug;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use url::Url;
use uuid::Uuid;
use vec1::Vec1;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection,
        provider::{
            IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind,
            IntegrationProviderSource,
        },
    },
    notification::{
        Notification, NotificationId, NotificationSource, NotificationSourceKind,
        NotificationStatus, service::NotificationPatch,
    },
    pii::Pii,
    realtime::RealtimeEventKind,
    third_party::{
        integrations::notion::{
            MentionChange, MentionedBlock, NotionBlock, NotionComment, NotionCommentMessage,
            NotionCommentParent, NotionPage, NotionUser, mention_change, parse_rich_text,
        },
        item::{
            ThirdPartyItem, ThirdPartyItemFromSource, ThirdPartyItemKind, ThirdPartyItemSourceKind,
        },
    },
    user::UserId,
};

use crate::{
    integrations::{
        notification::ThirdPartyNotificationSourceService, oauth2::AccessToken,
        third_party::ThirdPartyItemSourceService,
    },
    observability::attr,
    repository::{
        Repository, notification::NotificationRepository, third_party::ThirdPartyItemRepository,
    },
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        realtime::record_event,
    },
    utils::api::{ApiClient, ApiClientError},
};

pub mod oauth;
pub mod webhook;

static NOTION_BASE_URL: &str = "https://api.notion.com";
/// The comments, pages, blocks and users endpoints we use are stable in this
/// version.
static NOTION_VERSION: &str = "2022-06-28";

/// How far back the sync looks before its cursor: Notion rounds
/// `last_edited_time` to the minute.
const SYNC_CURSOR_MARGIN_MINUTES: i64 = 5;
/// How many pages edited since the cursor the sync reads.
const MAX_SEARCHED_PAGES: usize = 100;
/// How many blocks the sync reads, all pages together.
const MAX_WALKED_BLOCKS: usize = 1000;
/// How deep the sync walks nested blocks.
const MAX_BLOCK_DEPTH: usize = 3;
/// How often the sync re-reads the tracked discussions: a new comment does not
/// bump the page's `last_edited_time`, so search cannot find it.
const COMMENTS_RECONCILIATION_INTERVAL_MINUTES: i64 = 60;
/// Only discussions active in this window are re-read.
const RECONCILED_DISCUSSIONS_MAX_AGE_DAYS: i64 = 14;
const MAX_RECONCILED_DISCUSSIONS: usize = 30;

#[derive(Clone)]
pub struct NotionService {
    notion_base_url: String,
    notion_base_path: String,
    repository: Arc<Repository>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
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
    pub last_edited_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub properties: HashMap<String, Value>,
}

#[derive(Deserialize, Debug)]
struct RawNotionPageList {
    results: Vec<RawNotionPage>,
    #[serde(default)]
    has_more: bool,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RawNotionBlockList {
    results: Vec<RawNotionBlock>,
    #[serde(default)]
    has_more: bool,
    next_cursor: Option<String>,
}

impl From<RawNotionPage> for NotionPage {
    fn from(page: RawNotionPage) -> Self {
        NotionPage {
            title: page.title(),
            id: page.id,
            url: page.url,
            created_by: page.created_by.map(|user| user.id),
        }
    }
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

/// The parent of a block. Only pages and blocks are walked: a block in a
/// database row is under the row's page.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawNotionBlockParent {
    PageId {
        page_id: String,
    },
    BlockId {
        block_id: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct RawNotionBlock {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub parent: Option<RawNotionBlockParent>,
    pub created_by: Option<RawNotionUserReference>,
    #[serde(default)]
    pub last_edited_by: Option<RawNotionUserReference>,
    #[serde(default)]
    pub last_edited_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub in_trash: bool,
    #[serde(default)]
    pub has_children: bool,
    #[serde(flatten)]
    pub content: HashMap<String, Value>,
}

impl RawNotionBlock {
    pub fn is_removed(&self) -> bool {
        self.archived || self.in_trash
    }

    /// Nested pages and databases are pages of their own: the sync finds
    /// them through search.
    fn has_walkable_children(&self) -> bool {
        self.has_children && self.block_type != "child_page" && self.block_type != "child_database"
    }

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

/// How many parent blocks are read to find the page of a nested block.
const MAX_BLOCK_PARENT_HOPS: usize = 5;

/// Pages and users already read while handling one event, with one token.
#[derive(Default)]
pub struct NotionObjectCache {
    pages: HashMap<String, NotionPage>,
    users: HashMap<String, NotionUser>,
}

/// Whether a Notion call failed because the integration cannot see the
/// object (the page is not shared with it).
pub fn is_access_denied(error: &UniversalInboxError) -> bool {
    matches!(error, UniversalInboxError::Forbidden(_))
}

/// The user unsubscribed from the `tracked` discussion and none of the
/// comments since mentions them: `discussion` must not bring it back. The
/// Notion API cannot unsubscribe, so this is how Universal Inbox honors it.
pub fn is_unsubscribed_without_new_mention(
    existing_notification: Option<&Notification>,
    tracked: &NotionComment,
    discussion: &NotionComment,
    user_notion_id: &str,
) -> bool {
    existing_notification
        .is_some_and(|notification| notification.status == NotificationStatus::Unsubscribed)
        && !discussion
            .mentions_user_since(user_notion_id, tracked.latest_comment().last_edited_time)
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
        repository: Arc<Repository>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
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
            repository,
            integration_connection_service,
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
                debug!("Unable to fetch Notion user: {error:?}");
                NotionUser {
                    id: user_id.to_string(),
                    name: None,
                    avatar_url: None,
                }
            }
        }
    }

    async fn get_cached_page(
        &self,
        access_token: &AccessToken,
        page_id: &str,
        cache: &mut NotionObjectCache,
    ) -> Result<NotionPage, UniversalInboxError> {
        if let Some(page) = cache.pages.get(page_id) {
            return Ok(page.clone());
        }
        let page: NotionPage = self.get_page(access_token, page_id).await?.into();
        cache.pages.insert(page_id.to_string(), page.clone());
        Ok(page)
    }

    async fn get_cached_user(
        &self,
        access_token: &AccessToken,
        user_id: &str,
        cache: &mut NotionObjectCache,
    ) -> NotionUser {
        if let Some(user) = cache.users.get(user_id) {
            return user.clone();
        }
        let user = self.get_user(access_token, user_id).await;
        cache.users.insert(user_id.to_string(), user.clone());
        user
    }

    /// The id of the page a block belongs to, walking up its parent blocks.
    /// `None` when the block is not under a page within a few hops.
    async fn find_block_page_id(
        &self,
        access_token: &AccessToken,
        block: &RawNotionBlock,
    ) -> Result<Option<String>, UniversalInboxError> {
        let mut parent = block.parent.clone();
        for _ in 0..MAX_BLOCK_PARENT_HOPS {
            match parent {
                Some(RawNotionBlockParent::PageId { page_id }) => return Ok(Some(page_id)),
                Some(RawNotionBlockParent::BlockId { block_id }) => {
                    parent = self.get_block(access_token, &block_id).await?.parent;
                }
                Some(RawNotionBlockParent::Other) | None => return Ok(None),
            }
        }
        Ok(None)
    }

    /// Read a block that may mention a user, with the user's token. `None`
    /// when the block was deleted, archived or is no longer shared with the
    /// integration.
    pub async fn fetch_mention_block(
        &self,
        access_token: &AccessToken,
        block_id: &str,
        user_notion_id: &str,
        cache: &mut NotionObjectCache,
    ) -> Result<Option<NotionBlock>, UniversalInboxError> {
        let block = match self.get_block(access_token, block_id).await {
            Ok(block) if !block.is_removed() => block,
            Ok(_) => return Ok(None),
            Err(error) if is_access_denied(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let page = match self.find_block_page_id(access_token, &block).await {
            Ok(Some(page_id)) => self.get_cached_page(access_token, &page_id, cache).await,
            Ok(None) => {
                debug!("Notion block {block_id} is not under a page, ignoring it");
                return Ok(None);
            }
            Err(error) => Err(error),
        };
        let page = match page {
            Ok(page) => page,
            Err(error) if is_access_denied(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(
            self.build_mention_block(access_token, block, page, user_notion_id, cache)
                .await,
        ))
    }

    async fn build_mention_block(
        &self,
        access_token: &AccessToken,
        block: RawNotionBlock,
        page: NotionPage,
        user_notion_id: &str,
        cache: &mut NotionObjectCache,
    ) -> NotionBlock {
        let last_edited_by = match &block.last_edited_by {
            Some(user) => Some(self.get_cached_user(access_token, &user.id, cache).await),
            None => None,
        };

        NotionBlock {
            rich_text: parse_rich_text(&block.rich_text()),
            last_edited_time: block.last_edited_time.unwrap_or_else(Utc::now),
            id: block.id,
            block_type: block.block_type,
            page,
            last_edited_by,
            user_notion_id: Some(user_notion_id.to_string()),
        }
    }

    /// Pages visible to the integration, last edited first.
    async fn search_pages(
        &self,
        access_token: &AccessToken,
        start_cursor: Option<&str>,
    ) -> Result<RawNotionPageList, UniversalInboxError> {
        let mut body = serde_json::json!({
            "filter": { "property": "object", "value": "page" },
            "sort": { "direction": "descending", "timestamp": "last_edited_time" },
            "page_size": 100
        });
        if let Some(start_cursor) = start_cursor {
            body["start_cursor"] = Value::String(start_cursor.to_string());
        }
        self.build_notion_client(access_token)?
            .post(format!("{}/v1/search", self.notion_base_url), Some(&body))
            .await
            .map_err(|error| {
                anyhow!(error)
                    .context("Failed to search Notion pages")
                    .into()
            })
    }

    async fn list_block_children(
        &self,
        access_token: &AccessToken,
        block_id: &str,
        start_cursor: Option<&str>,
    ) -> Result<RawNotionBlockList, UniversalInboxError> {
        let url = format!(
            "{}/v1/blocks/{block_id}/children?page_size=100{}",
            self.notion_base_url,
            start_cursor
                .map(|cursor| format!("&start_cursor={cursor}"))
                .unwrap_or_default()
        );
        notion_get(
            &self.build_notion_client(access_token)?,
            url,
            "block children",
        )
        .await
    }

    /// The pages edited since `since`, newest first.
    async fn find_pages_edited_since(
        &self,
        access_token: &AccessToken,
        since: DateTime<Utc>,
    ) -> Result<Vec<RawNotionPage>, UniversalInboxError> {
        let mut pages = vec![];
        let mut start_cursor: Option<String> = None;
        loop {
            let result = self
                .search_pages(access_token, start_cursor.as_deref())
                .await?;
            for page in result.results {
                if page.last_edited_time.is_some_and(|time| time < since) {
                    return Ok(pages);
                }
                pages.push(page);
                if pages.len() >= MAX_SEARCHED_PAGES {
                    debug!("Reached the {MAX_SEARCHED_PAGES} Notion pages to sync");
                    return Ok(pages);
                }
            }
            if !result.has_more || result.next_cursor.is_none() {
                return Ok(pages);
            }
            start_cursor = result.next_cursor;
        }
    }

    /// The blocks of a page edited since `since`, walking nested blocks, the
    /// removed ones included. `walked_blocks` counts the blocks read across
    /// pages.
    async fn find_blocks_edited_since(
        &self,
        access_token: &AccessToken,
        page_id: &str,
        since: DateTime<Utc>,
        walked_blocks: &mut usize,
    ) -> Result<Vec<RawNotionBlock>, UniversalInboxError> {
        let mut edited_blocks = vec![];
        let mut parents = vec![(page_id.to_string(), 0)];
        while let Some((parent_id, depth)) = parents.pop() {
            let mut start_cursor: Option<String> = None;
            loop {
                let result = self
                    .list_block_children(access_token, &parent_id, start_cursor.as_deref())
                    .await?;
                for block in result.results {
                    *walked_blocks += 1;
                    if block.has_walkable_children()
                        && !block.is_removed()
                        && depth + 1 < MAX_BLOCK_DEPTH
                    {
                        parents.push((block.id.clone(), depth + 1));
                    }
                    if block.last_edited_time.is_some_and(|time| time >= since) {
                        edited_blocks.push(block);
                    }
                    if *walked_blocks >= MAX_WALKED_BLOCKS {
                        debug!("Reached the {MAX_WALKED_BLOCKS} Notion blocks to sync");
                        return Ok(edited_blocks);
                    }
                }
                if !result.has_more || result.next_cursor.is_none() {
                    break;
                }
                start_cursor = result.next_cursor;
            }
        }
        Ok(edited_blocks)
    }

    /// The blocks whose mention of the user changed since `since`: a new
    /// mention, a mention made again, or a removed mention.
    async fn fetch_changed_mentions(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        access_token: &AccessToken,
        connection: &IntegrationConnection,
        user_notion_id: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let current_span = tracing::Span::current();
        let pages = self.find_pages_edited_since(access_token, since).await?;
        current_span.set_attribute(attr::NOTION_SYNC_SEARCHED_PAGES_COUNT, pages.len() as i64);

        let mut cache = NotionObjectCache::default();
        let mut walked_blocks = 0;
        let mut items = vec![];
        for raw_page in pages {
            if walked_blocks >= MAX_WALKED_BLOCKS {
                break;
            }
            let page_id = raw_page.id.clone();
            let page: NotionPage = raw_page.into();
            cache.pages.insert(page_id.clone(), page.clone());
            let blocks = match self
                .find_blocks_edited_since(access_token, &page_id, since, &mut walked_blocks)
                .await
            {
                Ok(blocks) => blocks,
                Err(error) if is_access_denied(&error) => {
                    debug!("Skipping Notion page {page_id}: {error}");
                    continue;
                }
                Err(error) => return Err(error),
            };

            if blocks.is_empty() {
                continue;
            }
            // The stored blocks of the page, in one query
            let mut stored_blocks: HashMap<String, NotionBlock> = self
                .repository
                .find_third_party_items_for_source_ids(
                    executor,
                    ThirdPartyItemKind::NotionBlock,
                    &blocks
                        .iter()
                        .map(|block| block.id.clone())
                        .collect::<Vec<_>>(),
                    Some(connection.user_id),
                )
                .await?
                .into_iter()
                .filter(|item| item.integration_connection_id == connection.id)
                .map(|item| Ok((item.source_id.clone(), NotionBlock::try_from(item)?)))
                .collect::<Result<_, UniversalInboxError>>()?;
            for raw_block in blocks {
                let stored_block = stored_blocks.remove(&raw_block.id);
                let mentioned_block = if raw_block.is_removed() {
                    MentionedBlock::Removed
                } else {
                    MentionedBlock::Present {
                        is_mentioned: mentioned_user_ids_in_raw_rich_text(&raw_block.rich_text())
                            .iter()
                            .any(|id| id == user_notion_id),
                        is_edited_by_user: raw_block
                            .last_edited_by
                            .as_ref()
                            .is_some_and(|user| user.id == user_notion_id),
                    }
                };
                let was_mentioned = stored_block
                    .as_ref()
                    .map(|block| block.mentions_user(user_notion_id));
                match mention_change(mentioned_block, was_mentioned) {
                    // The sync notifies on creation only: a synced item would
                    // reset the notification as the user left it
                    MentionChange::Ignore | MentionChange::Refresh => continue,
                    MentionChange::Delete if mentioned_block == MentionedBlock::Removed => {
                        self.mark_block_notification_as_deleted(
                            executor,
                            &raw_block.id,
                            connection,
                        )
                        .await?;
                        continue;
                    }
                    // A removed mention makes a deleted notification
                    MentionChange::Notify | MentionChange::Delete => {}
                }

                let block = self
                    .build_mention_block(
                        access_token,
                        raw_block,
                        page.clone(),
                        user_notion_id,
                        &mut cache,
                    )
                    .await;
                items.push(block.into_third_party_item(connection.user_id, connection.id));
            }
        }
        current_span.set_attribute(attr::NOTION_SYNC_WALKED_BLOCKS_COUNT, walked_blocks as i64);
        Ok(items)
    }

    /// Delete the mention notification of a block moved to the trash.
    async fn mark_block_notification_as_deleted(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        block_id: &str,
        connection: &IntegrationConnection,
    ) -> Result<(), UniversalInboxError> {
        let Some(notification) = self
            .repository
            .get_notification_for_source_id(executor, block_id, connection.user_id)
            .await?
        else {
            return Ok(());
        };
        self.repository
            .update_notification(
                executor,
                notification.id,
                &NotificationPatch {
                    status: Some(NotificationStatus::Deleted),
                    ..Default::default()
                },
                connection.user_id,
            )
            .await?;
        record_event(
            connection.user_id,
            RealtimeEventKind::NotificationsChanged,
            Some(notification.id.0),
        );
        Ok(())
    }

    /// The tracked discussions with a comment newer than the stored one: a
    /// reply whose webhook was lost.
    async fn fetch_changed_discussions(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        access_token: &AccessToken,
        connection: &IntegrationConnection,
        user_notion_id: &str,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let oldest_activity = Utc::now() - TimeDelta::days(RECONCILED_DISCUSSIONS_MAX_AGE_DAYS);
        let mut tracked_discussions: Vec<NotionComment> = self
            .repository
            .find_third_party_items_for_user_id(
                executor,
                ThirdPartyItemKind::NotionComment,
                connection.user_id,
            )
            .await?
            .into_iter()
            .filter(|item| item.integration_connection_id == connection.id)
            .map(NotionComment::try_from)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|discussion| discussion.latest_comment().last_edited_time >= oldest_activity)
            .collect();
        tracked_discussions.sort_by_key(|discussion| {
            std::cmp::Reverse(discussion.latest_comment().last_edited_time)
        });
        tracked_discussions.truncate(MAX_RECONCILED_DISCUSSIONS);
        tracing::Span::current().set_attribute(
            attr::NOTION_SYNC_RECONCILED_DISCUSSIONS_COUNT,
            tracked_discussions.len() as i64,
        );

        // Discussions often share a page and authors
        let mut cache = NotionObjectCache::default();
        let mut items = vec![];
        for tracked in tracked_discussions {
            let parent_id = match &tracked.parent {
                NotionCommentParent::Page { page_id } => page_id,
                NotionCommentParent::Block { block_id } => block_id,
            };
            let discussion = match self
                .fetch_discussion(
                    access_token,
                    &tracked.page.id,
                    parent_id,
                    &tracked.discussion_id,
                    user_notion_id,
                    &mut cache,
                )
                .await
            {
                Ok(Some(discussion)) => discussion,
                // Deleted discussions are only handled by webhooks
                Ok(None) => continue,
                Err(error) if is_access_denied(&error) => {
                    debug!(
                        "Skipping Notion discussion {}: {error}",
                        tracked.discussion_id
                    );
                    continue;
                }
                Err(error) => return Err(error),
            };
            if discussion.latest_comment().last_edited_time
                <= tracked.latest_comment().last_edited_time
            {
                continue;
            }
            let existing_notification = self
                .repository
                .get_notification_for_source_id(
                    executor,
                    &tracked.discussion_id,
                    connection.user_id,
                )
                .await?;
            if is_unsubscribed_without_new_mention(
                existing_notification.as_ref(),
                &tracked,
                &discussion,
                user_notion_id,
            ) {
                continue;
            }
            items.push(discussion.into_third_party_item(connection.user_id, connection.id));
        }
        Ok(items)
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
        cache: &mut NotionObjectCache,
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

        let page = self.get_cached_page(access_token, page_id, cache).await?;
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

        let mut comments = vec![];
        for raw_comment in raw_comments {
            let author = self
                .get_cached_user(access_token, &raw_comment.created_by.id, cache)
                .await;
            comments.push(NotionCommentMessage {
                id: raw_comment.id,
                author,
                rich_text: parse_rich_text(&raw_comment.rich_text),
                created_time: raw_comment.created_time,
                last_edited_time: raw_comment.last_edited_time,
            });
        }

        Ok(Some(NotionComment {
            discussion_id: discussion_id.to_string(),
            page,
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

#[async_trait]
impl ThirdPartyNotificationSourceService<NotionBlock> for NotionService {
    async fn third_party_item_into_notification(
        &self,
        source: &NotionBlock,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let author = source
            .last_edited_by
            .as_ref()
            .and_then(|user| user.name.as_ref())
            .map(|name| name.expose().clone())
            .unwrap_or_else(|| "someone".to_string());

        // Mentions written by the user themselves are never saved
        let status = if source.is_mention_removed() {
            NotificationStatus::Deleted
        } else {
            NotificationStatus::Unread
        };
        Ok(Box::new(Notification {
            id: NotificationId(Uuid::new_v4()),
            title: format!("Mentioned by {author} on {}", source.page.title),
            status,
            created_at: source.last_edited_time,
            updated_at: source.last_edited_time,
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
        // The Notion public API cannot archive or mark a mention as read
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
        // There is nothing to unfollow on a mention
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

#[async_trait]
impl ThirdPartyItemSourceService<NotionComment> for NotionService {
    /// A safety net for the webhooks, which Notion delivers at most once. It
    /// returns only the items whose notification must change: the generic
    /// sync would otherwise reopen notifications the user already handled.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::USER_ID } = user_id.to_string(),
            { attr::NOTION_SYNC_SEARCHED_PAGES_COUNT } = tracing::field::Empty,
            { attr::NOTION_SYNC_WALKED_BLOCKS_COUNT } = tracing::field::Empty,
            { attr::NOTION_SYNC_RECONCILED_DISCUSSIONS_COUNT } = tracing::field::Empty
        )
    )]
    async fn fetch_items(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        last_sync_completed_at: Option<DateTime<Utc>>,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Notion, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot sync Notion notifications without an access token"))?;
        let Some(user_notion_id) = connection.provider_user_id.clone() else {
            return Err(anyhow!(
                "Cannot sync Notion notifications of a connection without a Notion user id"
            )
            .into());
        };
        let IntegrationProvider::Notion { config, context } = &connection.provider else {
            return Err(anyhow!("Expected a Notion integration connection").into());
        };

        let mut items = vec![];
        if config.sync_mentions_enabled {
            let since = last_sync_completed_at.unwrap_or(connection.created_at)
                - TimeDelta::minutes(SYNC_CURSOR_MARGIN_MINUTES);
            items.extend(
                self.fetch_changed_mentions(
                    executor,
                    &access_token,
                    &connection,
                    &user_notion_id,
                    since,
                )
                .await?,
            );
        }

        if config.sync_comments_enabled
            && let Some(context) = context
        {
            let now = Utc::now();
            let is_due = context.comments_reconciled_at.is_none_or(|reconciled_at| {
                now - reconciled_at >= TimeDelta::minutes(COMMENTS_RECONCILIATION_INTERVAL_MINUTES)
            });
            if is_due {
                items.extend(
                    self.fetch_changed_discussions(
                        executor,
                        &access_token,
                        &connection,
                        &user_notion_id,
                    )
                    .await?,
                );
                self.integration_connection_service
                    .read()
                    .await
                    .update_integration_connection_context(
                        executor,
                        connection.id,
                        IntegrationConnectionContext::Notion(
                            universal_inbox::integration_connection::integrations::notion::NotionContext {
                                comments_reconciled_at: Some(now),
                                ..context.clone()
                            },
                        ),
                    )
                    .await?;
            }
        }

        Ok(items)
    }

    fn is_sync_incremental(&self) -> bool {
        // A sync only reads what changed: never delete what it did not see
        true
    }

    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::NotionComment
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
        "/v1/blocks/{block_id}/children",
        "/v1/search",
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
    fn test_block_parent_and_edit_metadata() {
        let block: RawNotionBlock = serde_json::from_value(json!({
            "id": "block-id",
            "type": "paragraph",
            "parent": { "type": "block_id", "block_id": "parent-block-id" },
            "last_edited_by": { "object": "user", "id": "editor-id" },
            "last_edited_time": "2026-10-10T09:00:00.000Z",
            "in_trash": false,
            "paragraph": { "rich_text": [] }
        }))
        .unwrap();

        assert_eq!(
            block.parent,
            Some(RawNotionBlockParent::BlockId {
                block_id: "parent-block-id".to_string()
            })
        );
        assert_eq!(block.last_edited_by.as_ref().unwrap().id, "editor-id");
        assert!(!block.is_removed());

        let archived: RawNotionBlock = serde_json::from_value(json!({
            "id": "block-id",
            "type": "paragraph",
            "parent": { "type": "database_id", "database_id": "database-id" },
            "archived": true
        }))
        .unwrap();
        assert_eq!(archived.parent, Some(RawNotionBlockParent::Other));
        assert!(archived.is_removed());
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
