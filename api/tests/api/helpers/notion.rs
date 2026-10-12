use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use redis_apalis::AsyncCommands;
use reqwest::{Client, Response};
use ring::hmac;
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

use universal_inbox::{
    integration_connection::{
        IntegrationConnection,
        config::IntegrationConnectionConfig,
        integrations::notion::{NotionConfig, NotionContext},
        provider::IntegrationConnectionContext,
    },
    user::UserId,
};
use universal_inbox_api::{configuration::Settings, integrations::oauth2::AccessToken};

use crate::helpers::{
    TestedApp,
    auth::AuthenticatedApp,
    integration_connection::{OAuthCredentialFixture, create_and_mock_integration_connection},
};

/// Notion ids shared by the Notion tests: the connected user (`ME`, whose API
/// calls use `MY_TOKEN`), another user, and the page, block and discussion
/// the tests comment on.
pub const ME: &str = "notion-user-me";
pub const MY_TOKEN: &str = "notion-token-me";
pub const SOMEONE: &str = "notion-user-someone";
pub const PAGE_ID: &str = "3c612f56-fdd0-4a30-a4d6-bda7d7426309";
pub const BLOCK_ID: &str = "b1b2b3b4-0000-4000-8000-0000000000bb";
pub const DISCUSSION_ID: &str = "a2b6c3d4-0000-4000-8000-0000000000aa";

pub struct NotionTestConnection {
    pub workspace_id: String,
    pub connection: Box<IntegrationConnection>,
}

/// Connect `ME` to Notion in a workspace unique to the test, so the cached
/// workspace lookup of one test never answers for another.
pub async fn connect_me(
    app: &AuthenticatedApp,
    settings: &Settings,
    config: NotionConfig,
) -> NotionTestConnection {
    let workspace_id = uuid::Uuid::new_v4().to_string();
    let connection = create_notion_integration_connection(
        &app.app,
        app.user.id,
        settings,
        ME,
        MY_TOKEN,
        &workspace_id,
        config,
    )
    .await;
    NotionTestConnection {
        workspace_id,
        connection,
    }
}

/// Replace the mocked Notion API with the discussion `DISCUSSION_ID` on
/// `BLOCK_ID` (created by `block_created_by`) of `PAGE_ID` (created by
/// `SOMEONE`), holding `comments`.
pub async fn mock_my_discussion(
    app: &AuthenticatedApp,
    block_created_by: &str,
    comments: Vec<Value>,
) {
    app.app.notion_mock_server.reset().await;
    mock_notion_discussion(
        &app.app.notion_mock_server,
        MY_TOKEN,
        PAGE_ID,
        SOMEONE,
        BLOCK_ID,
        block_created_by,
        comments,
    )
    .await;
}

/// Counts the jobs pushed to the test's queue, whatever their state (see
/// `test_slack_webhook_message.rs`).
pub async fn pushed_jobs_count(app: &TestedApp) -> usize {
    app.redis_storage
        .get_connection()
        .clone()
        .hlen(app.redis_storage.get_config().job_data_hash())
        .await
        .expect("Failed to get jobs count")
}

/// Signing secret matching `api/config/test.toml::[integrations.notion].signing_secret`.
const TEST_NOTION_SIGNING_SECRET: &str = "notion-test-signing-secret-do-not-use-in-prod";

/// POST a Notion webhook event to `/hooks/notion/events` signed like Notion
/// does: `X-Notion-Signature: sha256=<hex HMAC-SHA256(body)>`.
pub async fn post_signed_notion_event(
    client: &Client,
    api_address: &str,
    payload: &Value,
) -> Response {
    let body = serde_json::to_vec(payload).expect("Failed to serialize Notion payload");
    let key = hmac::Key::new(hmac::HMAC_SHA256, TEST_NOTION_SIGNING_SECRET.as_bytes());
    let signature = format!("sha256={}", hex::encode(hmac::sign(&key, &body).as_ref()));

    client
        .post(format!("{api_address}hooks/notion/events"))
        .header("content-type", "application/json")
        .header("X-Notion-Signature", signature)
        .body(body)
        .send()
        .await
        .expect("Failed to execute request")
}

#[allow(clippy::too_many_arguments)]
pub fn notion_comment_event(
    workspace_id: &str,
    event_type: &str,
    comment_id: &str,
    author_id: &str,
    discussion_id: &str,
    page_id: &str,
    parent_block_id: &str,
    accessible_by: &[&str],
) -> Value {
    json!({
        "id": format!("event-{comment_id}"),
        "timestamp": "2026-10-10T09:00:00.000Z",
        "workspace_id": workspace_id,
        "workspace_name": "Acme",
        "subscription_id": "subscription-id",
        "integration_id": "integration-id",
        "type": event_type,
        "authors": [{ "id": author_id, "type": "person" }],
        "accessible_by": accessible_by
            .iter()
            .map(|id| json!({ "id": id, "type": "person" }))
            .collect::<Vec<_>>(),
        "attempt_number": 1,
        "entity": { "id": comment_id, "type": "comment" },
        "data": {
            "page_id": page_id,
            "parent": { "id": parent_block_id, "type": "block" },
            "discussion_id": discussion_id
        }
    })
}

/// A validated Notion connection of `workspace_id` for the Notion user
/// `notion_user_id`, whose API calls use `access_token`.
#[allow(clippy::too_many_arguments)]
pub async fn create_notion_integration_connection(
    app: &TestedApp,
    user_id: UserId,
    settings: &Settings,
    notion_user_id: &str,
    access_token: &str,
    workspace_id: &str,
    config: NotionConfig,
) -> Box<IntegrationConnection> {
    create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Notion(config),
        settings,
        OAuthCredentialFixture {
            access_token: AccessToken(access_token.to_string()),
            refresh_token: None,
            provider_user_id: Some(notion_user_id.to_string()),
            registered_oauth_scopes: vec![],
        },
        None,
        Some(IntegrationConnectionContext::Notion(NotionContext {
            workspace_id: workspace_id.to_string(),
            workspace_name: Some("Acme".to_string()),
            bot_id: format!("bot-{notion_user_id}"),
            user_display_name: None,
            user_email_address: None,
            comments_reconciled_at: None,
        })),
    )
    .await
}

pub fn text_segment(content: &str) -> Value {
    json!({
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

pub fn user_mention_segment(notion_user_id: &str, name: &str) -> Value {
    json!({
        "type": "mention",
        "mention": { "type": "user", "user": { "object": "user", "id": notion_user_id } },
        "annotations": {
            "bold": false, "italic": false, "strikethrough": false,
            "underline": false, "code": false, "color": "default"
        },
        "plain_text": format!("@{name}"),
        "href": null
    })
}

pub fn notion_comment(
    comment_id: &str,
    discussion_id: &str,
    parent_block_id: &str,
    author_id: &str,
    minute: u32,
    rich_text: Vec<Value>,
) -> Value {
    let time = format!("2026-10-10T09:{minute:02}:00.000Z");
    json!({
        "object": "comment",
        "id": comment_id,
        "parent": { "type": "block_id", "block_id": parent_block_id },
        "discussion_id": discussion_id,
        "created_time": time,
        "last_edited_time": time,
        "created_by": { "object": "user", "id": author_id },
        "rich_text": rich_text
    })
}

/// Mock the Notion API calls made with `access_token`: the comments of
/// `parent_block_id`, its page and block, and users.
#[allow(clippy::too_many_arguments)]
pub async fn mock_notion_discussion(
    mock_server: &MockServer,
    access_token: &str,
    page_id: &str,
    page_created_by: &str,
    parent_block_id: &str,
    block_created_by: &str,
    comments: Vec<Value>,
) {
    let authorization = format!("Bearer {access_token}");
    Mock::given(method("GET"))
        .and(path("/v1/comments"))
        .and(query_param("block_id", parent_block_id))
        .and(header("authorization", authorization.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "results": comments,
            "next_cursor": null,
            "has_more": false
        })))
        .mount(mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/pages/{page_id}")))
        .and(header("authorization", authorization.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "page",
            "id": page_id,
            "url": format!("https://www.notion.so/Roadmap-{}", page_id.replace('-', "")),
            "created_by": { "object": "user", "id": page_created_by },
            "properties": {
                "title": {
                    "id": "title",
                    "type": "title",
                    "title": [text_segment("Roadmap")]
                }
            }
        })))
        .mount(mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/blocks/{parent_block_id}")))
        .and(header("authorization", authorization.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "block",
            "id": parent_block_id,
            "type": "paragraph",
            "created_by": { "object": "user", "id": block_created_by },
            "paragraph": { "rich_text": [text_segment("The commented paragraph")] }
        })))
        .mount(mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex_users())
        .and(header("authorization", authorization.as_str()))
        .respond_with(|request: &wiremock::Request| {
            let user_id = request.url.path().trim_start_matches("/v1/users/");
            ResponseTemplate::new(200).set_body_json(json!({
                "object": "user",
                "id": user_id,
                "name": format!("User {user_id}"),
                "avatar_url": null,
                "type": "person"
            }))
        })
        .mount(mock_server)
        .await;
}

/// Mock every Notion API call made with `access_token` as not accessible
/// (the page is not shared with that user's integration).
pub async fn mock_notion_not_accessible(mock_server: &MockServer, access_token: &str) {
    Mock::given(header(
        "authorization",
        format!("Bearer {access_token}").as_str(),
    ))
    .respond_with(ResponseTemplate::new(404).set_body_json(json!({
        "object": "error",
        "status": 404,
        "code": "object_not_found",
        "message": "Could not find object"
    })))
    .mount(mock_server)
    .await;
}

/// Fail the `request_path` calls made with `access_token`, except the first
/// `request_path` call of the test, whatever its token: the triage (which
/// may use any connection) still reads the object, then the save of
/// `access_token`'s connection fails. A 400, as the API client retries a
/// 5xx for a while before failing.
pub async fn mock_notion_failure_after_triage(
    mock_server: &MockServer,
    request_path: &str,
    access_token: &str,
) {
    Mock::given(FailingAfterTriage {
        request_path: request_path.to_string(),
        authorization: format!("Bearer {access_token}"),
        calls: AtomicUsize::new(0),
    })
    .respond_with(ResponseTemplate::new(400).set_body_json(json!({
        "object": "error",
        "status": 400,
        "code": "validation_error",
        "message": "Invalid request"
    })))
    .with_priority(1)
    .mount(mock_server)
    .await;
}

struct FailingAfterTriage {
    request_path: String,
    authorization: String,
    calls: AtomicUsize,
}

impl wiremock::Match for FailingAfterTriage {
    fn matches(&self, request: &wiremock::Request) -> bool {
        if request.url.path() != self.request_path {
            return false;
        }
        let previous_calls = self.calls.fetch_add(1, Ordering::SeqCst);
        previous_calls > 0
            && request
                .headers
                .get("authorization")
                .is_some_and(|value| value == self.authorization.as_str())
    }
}

fn path_regex_users() -> wiremock::matchers::PathRegexMatcher {
    wiremock::matchers::path_regex(r"^/v1/users/[^/]+$")
}

pub fn notion_page_content_event(
    workspace_id: &str,
    event_id: &str,
    page_id: &str,
    block_ids: &[&str],
    accessible_by: &[&str],
) -> Value {
    json!({
        "id": event_id,
        "timestamp": "2026-10-10T09:00:00.000Z",
        "workspace_id": workspace_id,
        "workspace_name": "Acme",
        "subscription_id": "subscription-id",
        "integration_id": "integration-id",
        "type": "page.content_updated",
        "authors": [],
        "accessible_by": accessible_by
            .iter()
            .map(|id| json!({ "id": id, "type": "person" }))
            .collect::<Vec<_>>(),
        "attempt_number": 1,
        "entity": { "id": page_id, "type": "page" },
        "data": {
            "parent": { "id": "space-id", "type": "space" },
            "updated_blocks": block_ids
                .iter()
                .map(|id| json!({ "id": id, "type": "block" }))
                .collect::<Vec<_>>()
        }
    })
}

/// A `paragraph` block of `parent` (`json!({"type": "page_id", ...})` or
/// `json!({"type": "block_id", ...})`), last edited by `last_edited_by` at
/// 09:`minute`.
pub fn notion_paragraph_block(
    block_id: &str,
    parent: Value,
    last_edited_by: &str,
    minute: u32,
    rich_text: Vec<Value>,
) -> Value {
    json!({
        "object": "block",
        "id": block_id,
        "parent": parent,
        "type": "paragraph",
        "created_by": { "object": "user", "id": last_edited_by },
        "last_edited_by": { "object": "user", "id": last_edited_by },
        "last_edited_time": format!("2026-10-10T09:{minute:02}:00.000Z"),
        "archived": false,
        "in_trash": false,
        "paragraph": { "rich_text": rich_text }
    })
}

/// Mock the Notion API calls made with `access_token` to read mentioning
/// blocks: `blocks` (by their `id`), the page `page_id` and users.
pub async fn mock_notion_blocks(
    mock_server: &MockServer,
    access_token: &str,
    page_id: &str,
    blocks: Vec<Value>,
) {
    let authorization = format!("Bearer {access_token}");
    for block in blocks {
        let block_id = block["id"].as_str().unwrap().to_string();
        Mock::given(method("GET"))
            .and(path(format!("/v1/blocks/{block_id}")))
            .and(header("authorization", authorization.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(block))
            .mount(mock_server)
            .await;
    }
    mock_notion_page_and_users(mock_server, access_token, page_id).await;
}

pub async fn mock_notion_page_and_users(
    mock_server: &MockServer,
    access_token: &str,
    page_id: &str,
) {
    let authorization = format!("Bearer {access_token}");
    Mock::given(method("GET"))
        .and(path(format!("/v1/pages/{page_id}")))
        .and(header("authorization", authorization.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "page",
            "id": page_id,
            "url": format!("https://www.notion.so/Roadmap-{}", page_id.replace('-', "")),
            "created_by": { "object": "user", "id": "page-owner" },
            "properties": {
                "title": {
                    "id": "title",
                    "type": "title",
                    "title": [text_segment("Roadmap")]
                }
            }
        })))
        .mount(mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex_users())
        .and(header("authorization", authorization.as_str()))
        .respond_with(|request: &wiremock::Request| {
            let user_id = request.url.path().trim_start_matches("/v1/users/");
            ResponseTemplate::new(200).set_body_json(json!({
                "object": "user",
                "id": user_id,
                "name": format!("User {user_id}"),
                "avatar_url": null,
                "type": "person"
            }))
        })
        .mount(mock_server)
        .await;
}

/// Mock `block_id` as deleted (Notion answers 404), and the page and users.
pub async fn mock_notion_deleted_block(
    mock_server: &MockServer,
    access_token: &str,
    page_id: &str,
    block_id: &str,
) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/blocks/{block_id}")))
        .and(header(
            "authorization",
            format!("Bearer {access_token}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "object": "error",
            "status": 404,
            "code": "object_not_found",
            "message": "Could not find block"
        })))
        .mount(mock_server)
        .await;
    mock_notion_page_and_users(mock_server, access_token, page_id).await;
}

/// A page as returned by `POST /v1/search`.
pub fn notion_search_page(page_id: &str, last_edited_time: DateTime<Utc>) -> Value {
    json!({
        "object": "page",
        "id": page_id,
        "url": format!("https://www.notion.so/Roadmap-{}", page_id.replace('-', "")),
        "created_by": { "object": "user", "id": "page-owner" },
        "last_edited_time": last_edited_time.to_rfc3339(),
        "properties": {
            "title": { "id": "title", "type": "title", "title": [text_segment("Roadmap")] }
        }
    })
}

/// Mock `POST /v1/search` made with `access_token`: one result page with
/// `pages`, followed by another one when `next_cursor` is set.
pub async fn mock_notion_search(
    mock_server: &MockServer,
    access_token: &str,
    pages: Vec<Value>,
    next_cursor: Option<&str>,
) {
    Mock::given(method("POST"))
        .and(path("/v1/search"))
        .and(header(
            "authorization",
            format!("Bearer {access_token}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "results": pages,
            "next_cursor": next_cursor,
            "has_more": next_cursor.is_some()
        })))
        .mount(mock_server)
        .await;
}

/// Mock `GET /v1/blocks/{parent_id}/children` made with `access_token`.
pub async fn mock_notion_block_children(
    mock_server: &MockServer,
    access_token: &str,
    parent_id: &str,
    blocks: Vec<Value>,
) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/blocks/{parent_id}/children")))
        .and(header(
            "authorization",
            format!("Bearer {access_token}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "results": blocks,
            "next_cursor": null,
            "has_more": false
        })))
        .mount(mock_server)
        .await;
}
