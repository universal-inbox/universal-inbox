use std::str::FromStr;

use cucumber::{Parameter, given, when};
use reqwest::Client;
use serde_json::{Value, json};
use uuid::Uuid;

use universal_inbox::integration_connection::integrations::notion::NotionConfig;

use crate::{
    common::settings,
    helpers::{
        notion::{
            BLOCK_ID, DISCUSSION_ID, ME, MY_TOKEN, PAGE_ID, SOMEONE,
            create_notion_integration_connection, notion_comment_event, notion_page_content_event,
            post_signed_notion_event,
        },
        rest::delete_resource,
    },
    world::{ApiWorld, Response},
};

#[derive(Clone, Copy, Debug)]
pub enum NotionEvent {
    CommentCreated,
    PageContentUpdated,
    OfAnotherType,
    CommentOnAMalformedPageId,
    CommentInAMalformedParent,
    UpdateOfAMalformedBlockId,
}

impl Parameter for NotionEvent {
    const REGEX: &'static str = "comment event|page content event|event of another type|comment event whose page id is malformed|comment event whose parent block id is malformed|page content event whose updated block id is malformed";
    const NAME: &'static str = "notion_event";
}

impl FromStr for NotionEvent {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(match text {
            "comment event" => Self::CommentCreated,
            "page content event" => Self::PageContentUpdated,
            "event of another type" => Self::OfAnotherType,
            "comment event whose page id is malformed" => Self::CommentOnAMalformedPageId,
            "comment event whose parent block id is malformed" => Self::CommentInAMalformedParent,
            "page content event whose updated block id is malformed" => {
                Self::UpdateOfAMalformedBlockId
            }
            other => return Err(format!("Unknown Notion event {other:?}")),
        })
    }
}

impl NotionEvent {
    fn payload(self, workspace_id: &str) -> Value {
        match self {
            Self::CommentCreated => comment_event(workspace_id, "comment.created"),
            Self::PageContentUpdated => {
                notion_page_content_event(workspace_id, "event-1", PAGE_ID, &[BLOCK_ID], &[ME])
            }
            Self::OfAnotherType => comment_event(workspace_id, "page.locked"),
            Self::CommentOnAMalformedPageId => notion_comment_event(
                workspace_id,
                "comment.created",
                "comment-1",
                SOMEONE,
                DISCUSSION_ID,
                "../v1/users",
                BLOCK_ID,
                &[ME],
            ),
            Self::CommentInAMalformedParent => notion_comment_event(
                workspace_id,
                "comment.created",
                "comment-1",
                SOMEONE,
                DISCUSSION_ID,
                PAGE_ID,
                "../v1/users",
                &[ME],
            ),
            Self::UpdateOfAMalformedBlockId => notion_page_content_event(
                workspace_id,
                "event-1",
                PAGE_ID,
                &[BLOCK_ID, "../v1/users"],
                &[ME],
            ),
        }
    }
}

fn comment_event(workspace_id: &str, event_type: &str) -> Value {
    notion_comment_event(
        workspace_id,
        event_type,
        "comment-1",
        SOMEONE,
        DISCUSSION_ID,
        PAGE_ID,
        BLOCK_ID,
        &[ME],
    )
}

fn workspace_of(world: &ApiWorld) -> String {
    world
        .notion_webhook
        .workspace_id
        .clone()
        .expect("The account is not connected to Notion")
}

#[given("I am connected to Notion, in a workspace of its own")]
async fn i_am_connected_to_notion(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let workspace_id = Uuid::new_v4().to_string();
    let app = world.app.get().await;
    let connection = create_notion_integration_connection(
        app,
        user_id,
        &settings(),
        ME,
        MY_TOKEN,
        &workspace_id,
        NotionConfig::enabled(),
    )
    .await;
    world.notion_webhook.workspace_id = Some(workspace_id);
    world.notion_webhook.connection = Some(connection);
}

#[when("Notion sends the verification handshake")]
async fn notion_sends_the_handshake(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let response = Client::new()
        .post(format!("{}hooks/notion/events", app.api_address))
        .json(&json!({ "verification_token": "secret_handshake_token" }))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[when("Notion sends a comment event without a signature")]
async fn notion_sends_an_unsigned_event(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let response = Client::new()
        .post(format!("{}hooks/notion/events", app.api_address))
        .json(&comment_event(
            &Uuid::new_v4().to_string(),
            "comment.created",
        ))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[when("Notion sends a comment event with a wrong signature")]
async fn notion_sends_a_badly_signed_event(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let response = Client::new()
        .post(format!("{}hooks/notion/events", app.api_address))
        .header("X-Notion-Signature", "sha256=00ff")
        .json(&comment_event(
            &Uuid::new_v4().to_string(),
            "comment.created",
        ))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[when(expr = "Notion sends a {notion_event} of my workspace")]
async fn notion_sends_an_event_of_my_workspace(world: &mut ApiWorld, event: NotionEvent) {
    let workspace_id = workspace_of(world);
    let app = world.app.get().await;
    let response = post_signed_notion_event(
        &Client::new(),
        &app.api_address,
        &event.payload(&workspace_id),
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[when("Notion sends a comment event of a workspace that nobody connected")]
async fn notion_sends_an_event_of_an_unknown_workspace(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let response = post_signed_notion_event(
        &Client::new(),
        &app.api_address,
        &comment_event(&Uuid::new_v4().to_string(), "comment.created"),
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[when("I delete my Notion connection")]
async fn i_delete_my_notion_connection(world: &mut ApiWorld) {
    let id = world.notion_webhook.connection.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let _: Box<universal_inbox::integration_connection::IntegrationConnection> = delete_resource(
        &client,
        &app.api_address,
        "integration-connections",
        id.into(),
    )
    .await;
}
