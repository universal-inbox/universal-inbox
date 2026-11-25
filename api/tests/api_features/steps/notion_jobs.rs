use std::{str::FromStr, sync::Arc};

use apalis::prelude::Data;
use cucumber::{Parameter, gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::notion::NotionConfig,
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, service::NotificationPatch,
    },
    third_party::{integrations::notion::to_plain_text, item::ThirdPartyItemData},
};
use universal_inbox_api::{
    integrations::notion::webhook::NotionWebhookEvent,
    jobs::notion::{
        NotionWebhookEventJob, handle_notion_comment_event, handle_notion_page_content_event,
        handle_notion_webhook_event,
    },
    universal_inbox::audit::actor_context::with_actor,
};

use crate::{
    common::settings,
    helpers::{
        notification::list_notifications,
        notion::{
            BLOCK_ID, DISCUSSION_ID, ME, MY_TOKEN, PAGE_ID, SOMEONE,
            create_notion_integration_connection, mock_notion_blocks, mock_notion_deleted_block,
            mock_notion_discussion, mock_notion_failure_after_triage, mock_notion_not_accessible,
            notion_comment, notion_comment_event, notion_page_content_event,
            notion_paragraph_block, notion_webhook_actor, text_segment, user_mention_segment,
        },
        set_audit_actor,
    },
    steps::status::UserStatus,
    world::ApiWorld,
};

const SOMEONE_TOKEN: &str = "notion-token-someone";
const TABLE_BLOCK_ID: &str = "c1c2c3c4-0000-4000-8000-0000000000cc";

#[derive(Clone, Copy, Debug)]
pub enum Author {
    Me,
    Someone,
}

impl Parameter for Author {
    const REGEX: &'static str = "me|someone";
    const NAME: &'static str = "author";
}

impl FromStr for Author {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "me" => Ok(Self::Me),
            "someone" => Ok(Self::Someone),
            other => Err(format!("Unknown author {other:?}")),
        }
    }
}

impl Author {
    fn id(self) -> &'static str {
        match self {
            Self::Me => ME,
            Self::Someone => SOMEONE,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Audience {
    Me,
    Someone,
    SomeoneAndMe,
}

impl Parameter for Audience {
    const REGEX: &'static str = "me only|someone only|someone and me";
    const NAME: &'static str = "audience";
}

impl FromStr for Audience {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "me only" => Ok(Self::Me),
            "someone only" => Ok(Self::Someone),
            "someone and me" => Ok(Self::SomeoneAndMe),
            other => Err(format!("Unknown audience {other:?}")),
        }
    }
}

impl Audience {
    fn ids(self) -> Vec<&'static str> {
        match self {
            Self::Me => vec![ME],
            Self::Someone => vec![SOMEONE],
            Self::SomeoneAndMe => vec![SOMEONE, ME],
        }
    }
}

fn rich_text(text: &str) -> Vec<Value> {
    let mut segments = Vec::new();
    for (index, part) in text.split("@me").enumerate() {
        if index > 0 {
            segments.push(user_mention_segment(ME, "Me"));
        }
        if !part.is_empty() {
            segments.push(text_segment(part));
        }
    }
    segments
}

fn in_page() -> Value {
    json!({ "type": "page_id", "page_id": PAGE_ID })
}

fn workspace_of(world: &ApiWorld) -> String {
    world
        .notion_webhook
        .workspace_id
        .clone()
        .expect("The account is not connected to Notion")
}

async fn connect(world: &mut ApiWorld, config: NotionConfig) {
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
        config,
    )
    .await;
    world.notion_webhook.workspace_id = Some(workspace_id);
    world.notion_webhook.connection = Some(connection);
}

#[given("I am connected to Notion, with its comment sync disabled")]
async fn connected_without_comments(world: &mut ApiWorld) {
    connect(
        world,
        NotionConfig {
            sync_comments_enabled: false,
            sync_mentions_enabled: true,
        },
    )
    .await;
}

#[given("I am connected to Notion, with its mention sync disabled")]
async fn connected_without_mentions(world: &mut ApiWorld) {
    connect(
        world,
        NotionConfig {
            sync_comments_enabled: true,
            sync_mentions_enabled: false,
        },
    )
    .await;
}

#[given("the other user and I are connected to the same Notion workspace")]
async fn both_are_connected(world: &mut ApiWorld) {
    let other_user_id = world.other_session.as_ref().expect("No other user").user.id;
    let user_id = world.user().id;
    let workspace_id = Uuid::new_v4().to_string();
    let app = world.app.get().await;
    create_notion_integration_connection(
        app,
        other_user_id,
        &settings(),
        SOMEONE,
        SOMEONE_TOKEN,
        &workspace_id,
        NotionConfig::enabled(),
    )
    .await;
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

async fn serve_comments(world: &mut ApiWorld, page_by: &str, block_by: &str, comments: Vec<Value>) {
    let app = world.app.get().await;
    app.notion_mock_server.reset().await;
    mock_notion_discussion(
        &app.notion_mock_server,
        MY_TOKEN,
        PAGE_ID,
        page_by,
        BLOCK_ID,
        block_by,
        comments,
    )
    .await;
}

fn comments_of(step: &Step) -> Vec<Value> {
    step.table
        .as_ref()
        .expect("The comments must be given as a table")
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let author = match row[1].as_str() {
                "me" => ME,
                "someone" => SOMEONE,
                other => panic!("Unknown author {other:?}"),
            };
            notion_comment(
                &row[0],
                DISCUSSION_ID,
                BLOCK_ID,
                author,
                row[2].parse().expect("The minute must be a number"),
                rich_text(&row[3]),
            )
        })
        .collect()
}

#[given("Notion now serves a discussion with these comments:")]
#[when("Notion now serves a discussion with these comments:")]
async fn notion_serves_comments(world: &mut ApiWorld, step: &Step) {
    serve_comments(world, SOMEONE, SOMEONE, comments_of(step)).await;
}

#[given(
    expr = "Notion serves a discussion on a page created by {author} and a block created by {author}, with these comments:"
)]
async fn notion_serves_comments_of_creators(
    world: &mut ApiWorld,
    page_by: Author,
    block_by: Author,
    step: &Step,
) {
    serve_comments(world, page_by.id(), block_by.id(), comments_of(step)).await;
}

#[given(
    expr = "Notion serves, to the token {string}, a comment by a colleague that mentions someone and me"
)]
#[when(
    expr = "Notion serves, to the token {string}, a comment by a colleague that mentions someone and me"
)]
async fn notion_serves_a_comment_mentioning_both(world: &mut ApiWorld, token: String) {
    let app = world.app.get().await;
    mock_notion_discussion(
        &app.notion_mock_server,
        &token,
        PAGE_ID,
        "page-owner",
        BLOCK_ID,
        "block-owner",
        vec![notion_comment(
            "comment-1",
            DISCUSSION_ID,
            BLOCK_ID,
            "a-colleague",
            5,
            vec![
                user_mention_segment(SOMEONE, "Someone"),
                user_mention_segment(ME, "Me"),
            ],
        )],
    )
    .await;
}

#[given(
    expr = "Notion serves, to the token {string}, a block by a colleague that mentions someone and me"
)]
#[when(
    expr = "Notion serves, to the token {string}, a block by a colleague that mentions someone and me"
)]
async fn notion_serves_a_block_mentioning_both(world: &mut ApiWorld, token: String) {
    let app = world.app.get().await;
    mock_notion_blocks(
        &app.notion_mock_server,
        &token,
        PAGE_ID,
        vec![notion_paragraph_block(
            BLOCK_ID,
            in_page(),
            "a-colleague",
            5,
            vec![
                user_mention_segment(SOMEONE, "Someone"),
                user_mention_segment(ME, "Me"),
            ],
        )],
    )
    .await;
}

#[given(expr = "Notion does not share the page with the token {string}")]
async fn notion_hides_the_page(world: &mut ApiWorld, token: String) {
    let app = world.app.get().await;
    mock_notion_not_accessible(&app.notion_mock_server, &token).await;
}

#[given(expr = "Notion fails the comment requests of the token {string}, after the access check")]
async fn notion_fails_comments(world: &mut ApiWorld, token: String) {
    let app = world.app.get().await;
    mock_notion_failure_after_triage(&app.notion_mock_server, "/v1/comments", &token).await;
}

#[given(expr = "Notion fails the block requests of the token {string}, after the access check")]
async fn notion_fails_blocks(world: &mut ApiWorld, token: String) {
    let app = world.app.get().await;
    mock_notion_failure_after_triage(
        &app.notion_mock_server,
        &format!("/v1/blocks/{BLOCK_ID}"),
        &token,
    )
    .await;
}

async fn serve_blocks(world: &mut ApiWorld, blocks: Vec<Value>) {
    let app = world.app.get().await;
    app.notion_mock_server.reset().await;
    mock_notion_blocks(&app.notion_mock_server, MY_TOKEN, PAGE_ID, blocks).await;
}

#[given(
    expr = "Notion now serves a block last edited by {author} at minute {int}, reading {string}, with a mention of me"
)]
#[when(
    expr = "Notion now serves a block last edited by {author} at minute {int}, reading {string}, with a mention of me"
)]
async fn notion_serves_a_mentioning_block(
    world: &mut ApiWorld,
    author: Author,
    minute: u32,
    text: String,
) {
    let block = notion_paragraph_block(
        BLOCK_ID,
        in_page(),
        author.id(),
        minute,
        vec![text_segment(&text), user_mention_segment(ME, "Me")],
    );
    serve_blocks(world, vec![block]).await;
}

#[given(
    expr = "Notion now serves a block last edited by someone at minute {int}, reading {string}, without any mention"
)]
#[when(
    expr = "Notion now serves a block last edited by someone at minute {int}, reading {string}, without any mention"
)]
async fn notion_serves_a_plain_block(world: &mut ApiWorld, minute: u32, text: String) {
    let block = notion_paragraph_block(
        BLOCK_ID,
        in_page(),
        SOMEONE,
        minute,
        vec![text_segment(&text)],
    );
    serve_blocks(world, vec![block]).await;
}

#[given(
    "Notion serves a table whose row, last edited by someone, holds \"Owner\" and a mention of me"
)]
async fn notion_serves_a_table(world: &mut ApiWorld) {
    let table = json!({
        "object": "block",
        "id": TABLE_BLOCK_ID,
        "parent": in_page(),
        "type": "table",
        "table": { "table_width": 2 }
    });
    let row = json!({
        "object": "block",
        "id": BLOCK_ID,
        "parent": { "type": "block_id", "block_id": TABLE_BLOCK_ID },
        "type": "table_row",
        "last_edited_by": { "object": "user", "id": SOMEONE },
        "last_edited_time": "2026-10-10T09:05:00.000Z",
        "table_row": { "cells": [[text_segment("Owner")], [user_mention_segment(ME, "Me")]] }
    });
    serve_blocks(world, vec![table, row]).await;
}

#[given(expr = "Notion serves {int} blocks without any mention")]
async fn notion_serves_many_blocks(world: &mut ApiWorld, count: usize) {
    let blocks = block_ids(count)
        .iter()
        .map(|block_id| {
            notion_paragraph_block(block_id, in_page(), SOMEONE, 5, vec![text_segment("x")])
        })
        .collect();
    serve_blocks(world, blocks).await;
}

#[when("Notion now reports that block as deleted")]
async fn notion_reports_the_block_deleted(world: &mut ApiWorld) {
    let app = world.app.get().await;
    app.notion_mock_server.reset().await;
    mock_notion_deleted_block(&app.notion_mock_server, MY_TOKEN, PAGE_ID, BLOCK_ID).await;
}

fn block_ids(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("d0d0d0d0-0000-4000-8000-{index:012}"))
        .collect()
}

#[when("Notion answers again")]
async fn notion_answers_again(world: &mut ApiWorld) {
    let app = world.app.get().await;
    app.notion_mock_server.reset().await;
}

fn comment_event_of(
    world: &ApiWorld,
    kind: &str,
    comment_id: &str,
    author: &str,
    audience: Audience,
) -> Value {
    notion_comment_event(
        &workspace_of(world),
        kind,
        comment_id,
        author,
        DISCUSSION_ID,
        PAGE_ID,
        BLOCK_ID,
        &audience.ids(),
    )
}

async fn handle_comment(world: &mut ApiWorld, event: Value) {
    let event: NotionWebhookEvent = serde_json::from_value(event).unwrap();
    let app = world.app.get().await;
    let service = app.notification_service.read().await;
    let mut transaction = service.begin().await.unwrap();
    set_audit_actor(&mut transaction, &notion_webhook_actor()).await;
    handle_notion_comment_event(
        &mut transaction,
        &event,
        app.notification_service.clone(),
        app.integration_connection_service.clone(),
        app.third_party_item_service.clone(),
    )
    .await
    .unwrap()
    .into_result()
    .unwrap();
    transaction
        .commit()
        .await
        .expect("Failed to commit transaction");
}

#[when(
    expr = "the Notion job handles the comment {string} written by {author}, visible to {audience}"
)]
async fn the_job_handles_a_comment(
    world: &mut ApiWorld,
    comment_id: String,
    author: Author,
    audience: Audience,
) {
    let event = comment_event_of(world, "comment.created", &comment_id, author.id(), audience);
    handle_comment(world, event).await;
}

#[when(
    expr = "the Notion job handles the comment {string} written by {string}, visible to {audience}"
)]
async fn the_job_handles_a_comment_of_a_colleague(
    world: &mut ApiWorld,
    comment_id: String,
    author: String,
    audience: Audience,
) {
    let event = comment_event_of(world, "comment.created", &comment_id, &author, audience);
    handle_comment(world, event).await;
}

#[when(expr = "the Notion job handles the deletion of the comment {string}")]
async fn the_job_handles_a_deletion(world: &mut ApiWorld, comment_id: String) {
    let event = comment_event_of(world, "comment.deleted", &comment_id, SOMEONE, Audience::Me);
    handle_comment(world, event).await;
}

#[when(expr = "the Notion job handles the comment {string} of a workspace that nobody connected")]
async fn the_job_handles_a_comment_of_another_workspace(world: &mut ApiWorld, comment_id: String) {
    let event = notion_comment_event(
        &Uuid::new_v4().to_string(),
        "comment.created",
        &comment_id,
        SOMEONE,
        DISCUSSION_ID,
        PAGE_ID,
        BLOCK_ID,
        &[ME],
    );
    handle_comment(world, event).await;
}

async fn handle_blocks(world: &mut ApiWorld, block_ids: &[&str], audience: Audience) {
    let event: NotionWebhookEvent = serde_json::from_value(notion_page_content_event(
        &workspace_of(world),
        &format!("event-{}", Uuid::new_v4()),
        PAGE_ID,
        block_ids,
        &audience.ids(),
    ))
    .unwrap();
    let app = world.app.get().await;
    let service = app.notification_service.read().await;
    let mut transaction = service.begin().await.unwrap();
    set_audit_actor(&mut transaction, &notion_webhook_actor()).await;
    handle_notion_page_content_event(
        &mut transaction,
        &event,
        app.notification_service.clone(),
        app.integration_connection_service.clone(),
        app.third_party_item_service.clone(),
    )
    .await
    .unwrap()
    .into_result()
    .unwrap();
    transaction
        .commit()
        .await
        .expect("Failed to commit transaction");
}

#[when(expr = "the Notion job handles an update of the block, visible to {audience}")]
async fn the_job_handles_a_block_update(world: &mut ApiWorld, audience: Audience) {
    handle_blocks(world, &[BLOCK_ID], audience).await;
}

#[when(expr = "the Notion job handles an update that names the block twice, visible to {audience}")]
async fn the_job_handles_a_repeated_block(world: &mut ApiWorld, audience: Audience) {
    handle_blocks(world, &[BLOCK_ID, BLOCK_ID], audience).await;
}

#[when(expr = "the Notion job handles an update of {int} blocks, visible to {audience}")]
async fn the_job_handles_many_blocks(world: &mut ApiWorld, count: usize, audience: Audience) {
    let ids = block_ids(count);
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    handle_blocks(world, &ids, audience).await;
}

async fn run_the_worker_job(world: &mut ApiWorld, event: Value) {
    world.notion_webhook.last_job = Some(event.clone());
    let app = world.app.get().await;
    let result = with_actor(
        notion_webhook_actor(),
        handle_notion_webhook_event(
            NotionWebhookEventJob(serde_json::from_value(event).unwrap()),
            Data::new(Arc::clone(&app.notification_service)),
            Data::new(Arc::clone(&app.integration_connection_service)),
            Data::new(Arc::clone(&app.third_party_item_service)),
        ),
    )
    .await;
    world.notion_webhook.failure = result.err().map(|err| err.to_string());
}

#[when(
    expr = "the Notion worker runs the job of the comment {string} written by {string}, visible to {audience}"
)]
async fn the_worker_runs_a_comment_job(
    world: &mut ApiWorld,
    comment_id: String,
    author: String,
    audience: Audience,
) {
    let event = comment_event_of(world, "comment.created", &comment_id, &author, audience);
    run_the_worker_job(world, event).await;
}

#[when(expr = "the Notion worker runs the job of an update of the block, visible to {audience}")]
async fn the_worker_runs_a_block_job(world: &mut ApiWorld, audience: Audience) {
    let event = notion_page_content_event(
        &workspace_of(world),
        "event-1",
        PAGE_ID,
        &[BLOCK_ID],
        &audience.ids(),
    );
    run_the_worker_job(world, event).await;
}

#[when("the Notion worker runs the same job again")]
async fn the_worker_runs_it_again(world: &mut ApiWorld) {
    let event = world
        .notion_webhook
        .last_job
        .clone()
        .expect("No job ran yet");
    run_the_worker_job(world, event).await;
}

async fn list_for(world: &mut ApiWorld, statuses: Vec<NotificationStatus>, other: bool) {
    let client = if other {
        world.other_session.as_ref().unwrap().client.clone()
    } else {
        world.client()
    };
    let app = world.app.get().await;
    world.notion_webhook.listing = list_notifications(
        &client,
        &app.api_address,
        statuses,
        false,
        None,
        Some(NotificationSourceKind::Notion),
        false,
    )
    .await;
}

fn all_statuses() -> Vec<NotificationStatus> {
    vec![
        NotificationStatus::Unread,
        NotificationStatus::Read,
        NotificationStatus::Deleted,
        NotificationStatus::Unsubscribed,
    ]
}

#[when("I look at my unread Notion notifications")]
async fn i_look_at_unread(world: &mut ApiWorld) {
    list_for(world, vec![], false).await;
}

#[when("I look at my Notion notifications, whatever their status")]
async fn i_look_at_all(world: &mut ApiWorld) {
    list_for(world, all_statuses(), false).await;
}

#[when("the other user looks at their Notion notifications, whatever their status")]
async fn the_other_user_looks(world: &mut ApiWorld) {
    list_for(world, all_statuses(), true).await;
}

#[when(expr = "I set that Notion notification as {status}")]
async fn i_set_that_notification(world: &mut ApiWorld, status: UserStatus) {
    let id = only(world).id;
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    universal_inbox_api::repository::notification::NotificationRepository::update_notification(
        &*app.repository,
        &mut transaction,
        id,
        &NotificationPatch {
            status: Some(status.0),
            ..Default::default()
        },
        user_id,
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

#[when("I enable the comment sync of my Notion connection")]
async fn i_enable_comment_sync(world: &mut ApiWorld) {
    let id = world.notion_webhook.connection.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .put(format!(
            "{}integration-connections/{id}/config",
            app.api_address
        ))
        .json(&IntegrationConnectionConfig::Notion(NotionConfig::enabled()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

fn only(world: &ApiWorld) -> &Notification {
    assert_eq!(
        world.notion_webhook.listing.len(),
        1,
        "{:?}",
        world.notion_webhook.listing
    );
    &world.notion_webhook.listing[0]
}

#[then("I see no Notion notification")]
fn i_have_none(world: &mut ApiWorld) {
    assert!(
        world.notion_webhook.listing.is_empty(),
        "{:?}",
        world.notion_webhook.listing
    );
}

#[then(expr = "I have {int} Notion notification(s) listed")]
fn i_have_n(world: &mut ApiWorld, count: usize) {
    assert_eq!(world.notion_webhook.listing.len(), count);
}

#[then(expr = "my only Notion notification is {status}")]
fn my_only_is(world: &mut ApiWorld, status: UserStatus) {
    assert_eq!(only(world).status, status.0);
}

#[then(expr = "that Notion notification is titled {string}")]
fn that_is_titled(world: &mut ApiWorld, title: String) {
    assert_eq!(
        only(world).title,
        title.replace("<someone>", &format!("User {SOMEONE}"))
    );
}

#[then(expr = "that Notion notification was updated at {string}")]
fn that_was_updated_at(world: &mut ApiWorld, at: String) {
    assert_eq!(only(world).updated_at.to_rfc3339(), at);
}

#[then("that Notion notification is the comment of the discussion, with its quoted content")]
fn that_is_the_comment(world: &mut ApiWorld) {
    let ThirdPartyItemData::NotionComment(comment) = &only(world).source_item.data else {
        panic!("Expected a NotionComment item");
    };
    assert_eq!(comment.discussion_id, DISCUSSION_ID);
    assert!(comment.quoted_content.is_some());
}

#[then(expr = "that Notion notification is the block of the page, reading {string}")]
fn that_is_the_block(world: &mut ApiWorld, text: String) {
    let ThirdPartyItemData::NotionBlock(block) = &only(world).source_item.data else {
        panic!("Expected a NotionBlock item");
    };
    assert_eq!(block.id, BLOCK_ID);
    assert_eq!(block.page.id, PAGE_ID);
    assert_eq!(to_plain_text(&block.rich_text), text);
}

#[then(expr = "that Notion notification is a table row of the page, reading {string}")]
fn that_is_a_table_row(world: &mut ApiWorld, text: String) {
    let ThirdPartyItemData::NotionBlock(block) = &only(world).source_item.data else {
        panic!("Expected a NotionBlock item");
    };
    assert_eq!(block.block_type, "table_row");
    assert_eq!(block.page.id, PAGE_ID);
    assert_eq!(to_plain_text(&block.rich_text), text);
}

#[then(expr = "Notion was asked to read {int} blocks")]
async fn notion_was_asked_for_blocks(world: &mut ApiWorld, count: usize) {
    let app = world.app.get().await;
    let reads = app
        .notion_mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path().starts_with("/v1/blocks/"))
        .count();
    assert_eq!(reads, count);
}

#[then(expr = "the job fails, mentioning {string}")]
fn the_job_fails(world: &mut ApiWorld, text: String) {
    let failure = world
        .notion_webhook
        .failure
        .as_ref()
        .expect("The job did not fail");
    assert!(failure.contains(&text), "The failure is {failure}");
}

#[then("the job succeeds")]
fn the_job_succeeds(world: &mut ApiWorld) {
    assert_eq!(world.notion_webhook.failure, None);
}
