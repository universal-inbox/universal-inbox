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
    integration_connection::{OAuthCredentialFixture, create_and_mock_integration_connection},
};

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

fn path_regex_users() -> wiremock::matchers::PathRegexMatcher {
    wiremock::matchers::path_regex(r"^/v1/users/[^/]+$")
}
