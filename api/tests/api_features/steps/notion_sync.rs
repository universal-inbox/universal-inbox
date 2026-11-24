use std::str::FromStr;

use chrono::{DateTime, TimeDelta, Utc};
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use universal_inbox::{
    integration_connection::integrations::notion::NotionConfig,
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationSyncSourceKind,
        service::NotificationPatch,
    },
    third_party::{
        integrations::notion::{
            NotionComment, NotionCommentMessage, NotionCommentParent, NotionPage, NotionUser,
            to_plain_text,
        },
        item::ThirdPartyItemData,
    },
};
use universal_inbox_api::{
    integrations::notion::{NotionService, webhook::NotionWebhookEvent},
    jobs::notion::handle_notion_page_content_event,
};

use crate::{
    common::settings,
    helpers::{
        TestedApp,
        notification::{
            create_notification_from_source_item, list_notifications, sync_notifications,
        },
        notion::{
            BLOCK_ID, DISCUSSION_ID, ME, MY_TOKEN, PAGE_ID, SOMEONE,
            create_notion_integration_connection, mock_notion_block_children, mock_notion_blocks,
            mock_notion_discussion, mock_notion_page_and_users, mock_notion_search,
            notion_page_content_event, notion_search_page, notion_webhook_actor, text_segment,
            user_mention_segment,
        },
        set_audit_actor,
    },
    state::WorkspacePage,
    steps::{
        integration_connections::stored_connection, notifications::update, status::UserStatus,
    },
    world::ApiWorld,
};

const OLDER_PAGE_ID: &str = "d0d0d0d0-0000-4000-8000-0000000000dd";
const SECOND_DISCUSSION_ID: &str = "a2b6c3d4-0000-4000-8000-0000000000ab";
const SECOND_BLOCK_ID: &str = "b1b2b3b4-0000-4000-8000-0000000000bc";
const ALL_STATUSES: [NotificationStatus; 4] = [
    NotificationStatus::Unread,
    NotificationStatus::Read,
    NotificationStatus::Deleted,
    NotificationStatus::Unsubscribed,
];

#[derive(Clone, Copy, Debug)]
pub struct Age(pub TimeDelta);

impl Parameter for Age {
    const REGEX: &'static str = r"\d+ (?:min|days?)";
    const NAME: &'static str = "age";
}

impl FromStr for Age {
    type Err = String;

    fn from_str(age: &str) -> Result<Self, Self::Err> {
        let (count, unit) = age.split_once(' ').ok_or("Unknown age")?;
        let count: i64 = count.parse().map_err(|_| "Unknown age")?;
        match unit {
            "min" => Ok(Self(TimeDelta::minutes(count))),
            "day" | "days" => Ok(Self(TimeDelta::days(count))),
            other => Err(format!("Unknown age unit {other:?}")),
        }
    }
}

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

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "me" => Ok(Self::Me),
            "someone" => Ok(Self::Someone),
            other => Err(format!("Unknown author {other:?}")),
        }
    }
}

impl Author {
    fn notion_id(self) -> &'static str {
        match self {
            Self::Me => ME,
            Self::Someone => SOMEONE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mention {
    Mentioning,
    NotMentioning,
}

impl Parameter for Mention {
    const REGEX: &'static str = "mentioning me|not mentioning me";
    const NAME: &'static str = "mention";
}

impl FromStr for Mention {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "mentioning me" => Ok(Self::Mentioning),
            "not mentioning me" => Ok(Self::NotMentioning),
            other => Err(format!("Unknown mention {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Discussion(pub &'static str);

impl Parameter for Discussion {
    const REGEX: &'static str = "first|second";
    const NAME: &'static str = "discussion";
}

impl FromStr for Discussion {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "first" => Ok(Self(DISCUSSION_ID)),
            "second" => Ok(Self(SECOND_DISCUSSION_ID)),
            other => Err(format!("Unknown discussion {other:?}")),
        }
    }
}

fn now(world: &mut ApiWorld) -> DateTime<Utc> {
    *world.notion_sync.clock.get_or_insert_with(Utc::now)
}

fn ago(world: &mut ApiWorld, age: Age) -> DateTime<Utc> {
    now(world) - age.0
}

fn paragraph(
    block_id: &str,
    last_edited_by: &str,
    last_edited_time: DateTime<Utc>,
    rich_text: Vec<Value>,
) -> Value {
    json!({
        "object": "block",
        "id": block_id,
        "parent": { "type": "page_id", "page_id": PAGE_ID },
        "type": "paragraph",
        "has_children": false,
        "last_edited_by": { "object": "user", "id": last_edited_by },
        "last_edited_time": last_edited_time.to_rfc3339(),
        "paragraph": { "rich_text": rich_text }
    })
}

fn mentioning_paragraph(block_id: &str, author: Author, time: DateTime<Utc>) -> Value {
    paragraph(
        block_id,
        author.notion_id(),
        time,
        vec![
            text_segment("Can you check "),
            user_mention_segment(ME, "Me"),
        ],
    )
}

fn page(world: &mut ApiWorld) -> &mut WorkspacePage {
    world
        .notion_sync
        .page
        .as_mut()
        .expect("Notion has no page in this scenario")
}

async fn connect(world: &mut ApiWorld, config: NotionConfig) {
    let user_id = world.user().id;
    let workspace_id = uuid::Uuid::new_v4().to_string();
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
    world.notion_sync.workspace_id = Some(workspace_id);
    world.notion_sync.connection = Some(connection);
}

async fn mount_notion(world: &mut ApiWorld) {
    let clock = now(world);
    let sync = &world.notion_sync;
    let page = sync
        .page
        .as_ref()
        .map(|page| (page.edited, page.blocks.clone()));
    let older_page = sync.older_page;
    let comments = sync.comments.clone();
    let app = world.app.get().await;
    let server = &app.notion_mock_server;
    server.reset().await;
    if let Some((edited, blocks)) = page {
        let mut results = vec![notion_search_page(PAGE_ID, clock - edited)];
        if let Some(older) = older_page {
            results.push(notion_search_page(OLDER_PAGE_ID, clock - older));
        }
        mock_notion_search(
            server,
            MY_TOKEN,
            results,
            older_page.map(|_| "next-search-page"),
        )
        .await;
        mock_notion_block_children(server, MY_TOKEN, PAGE_ID, blocks).await;
        mock_notion_page_and_users(server, MY_TOKEN, PAGE_ID).await;
    }
    if !comments.is_empty() {
        mock_notion_discussion(server, MY_TOKEN, PAGE_ID, SOMEONE, BLOCK_ID, ME, comments).await;
    }
}

async fn force_sync(world: &mut ApiWorld) {
    mount_notion(world).await;
    let user_id = world.user().id;
    let app = world.app.get().await;
    app.notification_service
        .read()
        .await
        .sync_notifications_with_transaction(NotificationSyncSourceKind::Notion, user_id, true)
        .await
        .unwrap();
}

async fn notion_notifications(world: &mut ApiWorld) -> Vec<Notification> {
    let client = world.client();
    let app = world.app.get().await;
    list_notifications(
        &client,
        &app.api_address,
        ALL_STATUSES.to_vec(),
        false,
        None,
        Some(NotificationSourceKind::Notion),
        false,
    )
    .await
}

async fn requests_to(world: &mut ApiWorld, path_prefix: &str) -> usize {
    let app = world.app.get().await;
    app.notion_mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path().starts_with(path_prefix))
        .count()
}

#[given("I am connected to Notion, syncing only my mentions")]
async fn connected_for_mentions(world: &mut ApiWorld) {
    connect(
        world,
        NotionConfig {
            sync_comments_enabled: false,
            sync_mentions_enabled: true,
        },
    )
    .await;
}

#[given("I am connected to Notion, syncing only the comments of my discussions")]
async fn connected_for_comments(world: &mut ApiWorld) {
    connect(
        world,
        NotionConfig {
            sync_comments_enabled: true,
            sync_mentions_enabled: false,
        },
    )
    .await;
}

#[given(expr = "Notion has the page \"Roadmap\", edited {age} ago")]
fn notion_has_a_page(world: &mut ApiWorld, age: Age) {
    world.notion_sync.page = Some(WorkspacePage {
        edited: age.0,
        blocks: vec![],
    });
}

#[given(expr = "that page holds a paragraph mentioning me, written by {author} {age} ago")]
fn the_page_holds_a_mentioning_paragraph(world: &mut ApiWorld, author: Author, age: Age) {
    let block = mentioning_paragraph(BLOCK_ID, author, ago(world, age));
    page(world).blocks.push(block);
}

#[given(expr = "that page holds a second paragraph mentioning me, written by {author} {age} ago")]
fn the_page_holds_a_second_mentioning_paragraph(world: &mut ApiWorld, author: Author, age: Age) {
    let block = mentioning_paragraph(SECOND_BLOCK_ID, author, ago(world, age));
    page(world).blocks.push(block);
}

#[given(expr = "that page holds a paragraph saying {string}, written by someone {age} ago")]
fn the_page_holds_a_paragraph(world: &mut ApiWorld, text: String, age: Age) {
    let block = paragraph(
        BLOCK_ID,
        SOMEONE,
        ago(world, age),
        vec![text_segment(&text)],
    );
    page(world).blocks.push(block);
}

#[given(
    expr = "that page holds the paragraph mentioning me, written by someone {age} ago, moved to the trash"
)]
fn the_page_holds_a_trashed_paragraph(world: &mut ApiWorld, age: Age) {
    let mut block = mentioning_paragraph(BLOCK_ID, Author::Someone, ago(world, age));
    block["in_trash"] = json!(true);
    page(world).blocks.push(block);
}

#[given(
    expr = "Notion lists an older page, edited {age} ago, after that page, with more pages to come"
)]
fn notion_lists_an_older_page(world: &mut ApiWorld, age: Age) {
    world.notion_sync.older_page = Some(age.0);
}

#[given(
    expr = "I already track the Notion discussion {discussion}, whose first comment was written {age} ago, and I have read it"
)]
async fn i_already_track_a_discussion(world: &mut ApiWorld, discussion: Discussion, age: Age) {
    let first_comment_time = ago(world, age);
    let user_id = world.user().id;
    let connection_id = world.notion_sync.connection.as_ref().unwrap().id;
    let tracked = NotionComment {
        discussion_id: discussion.0.to_string(),
        page: NotionPage {
            id: PAGE_ID.to_string(),
            title: "Roadmap".to_string(),
            url: "https://www.notion.so/Roadmap-3c612f56".parse().unwrap(),
            created_by: Some(SOMEONE.to_string()),
        },
        parent: NotionCommentParent::Block {
            block_id: BLOCK_ID.to_string(),
        },
        quoted_content: None,
        parent_block_created_by: Some(ME.to_string()),
        comments: vec1::vec1![NotionCommentMessage {
            id: "comment-1".to_string(),
            author: NotionUser {
                id: SOMEONE.to_string(),
                name: None,
                avatar_url: None,
            },
            rich_text: vec![],
            created_time: first_comment_time,
            last_edited_time: first_comment_time,
        }],
        user_notion_id: Some(ME.to_string()),
    };
    let app = world.app.get().await;
    let notification = create_notification_from_source_item::<NotionComment, NotionService>(
        app,
        discussion.0.to_string(),
        ThirdPartyItemData::NotionComment(Box::new(tracked)),
        app.notification_service.read().await.notion_service.clone(),
        user_id,
        connection_id,
    )
    .await;
    world
        .notifications
        .by_name
        .insert(format!("discussion {}", discussion.0), *notification);
    mark_read(world, &format!("discussion {}", discussion.0)).await;
}

#[given(
    expr = "Notion's discussion {discussion} holds the comment {string}, written by someone {age} ago"
)]
fn the_discussion_holds_a_comment(
    world: &mut ApiWorld,
    discussion: Discussion,
    text: String,
    age: Age,
) {
    push_comment(world, discussion, &text, age, Mention::NotMentioning);
}

#[given(
    expr = "Notion's discussion {discussion} holds a reply {string}, written by someone {age} ago, {mention}"
)]
fn the_discussion_holds_a_reply(
    world: &mut ApiWorld,
    discussion: Discussion,
    text: String,
    age: Age,
    mention: Mention,
) {
    push_comment(world, discussion, &text, age, mention);
}

fn push_comment(
    world: &mut ApiWorld,
    discussion: Discussion,
    text: &str,
    age: Age,
    mention: Mention,
) {
    let time = ago(world, age);
    let comments = &mut world.notion_sync.comments;
    let id = format!("comment-{}", text.trim().replace(' ', "-"));
    let mut rich_text = vec![text_segment(text)];
    if mention == Mention::Mentioning {
        rich_text.push(user_mention_segment(ME, "Me"));
    }
    comments.push(json!({
        "object": "comment",
        "id": id,
        "parent": { "type": "block_id", "block_id": BLOCK_ID },
        "discussion_id": discussion.0,
        "created_time": time.to_rfc3339(),
        "last_edited_time": time.to_rfc3339(),
        "created_by": { "object": "user", "id": SOMEONE },
        "rich_text": rich_text
    }));
}

#[given(expr = "I unsubscribe from my notification of the Notion discussion {discussion}")]
async fn i_unsubscribe_from_the_discussion(world: &mut ApiWorld, discussion: Discussion) {
    update(
        world,
        &format!("discussion {}", discussion.0),
        NotificationPatch {
            status: Some(NotificationStatus::Unsubscribed),
            ..Default::default()
        },
    )
    .await;
}

async fn mark_read(world: &mut ApiWorld, name: &str) {
    update(
        world,
        name,
        NotificationPatch {
            status: Some(NotificationStatus::Read),
            ..Default::default()
        },
    )
    .await;
}

#[when("I sync my Notion notifications")]
async fn i_sync_my_notion_notifications(world: &mut ApiWorld) {
    mount_notion(world).await;
    let client = world.client();
    let app = world.app.get().await;
    sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Notion),
        false,
    )
    .await;
}

#[given("I force a sync of my Notion notifications, ignoring the minimum interval between syncs")]
#[when("I force a sync of my Notion notifications, ignoring the minimum interval between syncs")]
async fn i_force_a_sync(world: &mut ApiWorld) {
    force_sync(world).await;
}

#[given("I mark my Notion notifications as read")]
#[when("I mark my Notion notifications as read")]
async fn i_mark_my_notifications_as_read(world: &mut ApiWorld) {
    for notification in notion_notifications(world).await {
        world
            .notifications
            .by_name
            .insert(notification.id.to_string(), notification.clone());
        mark_read(world, &notification.id.to_string()).await;
    }
}

#[given("the Notion webhook reported that mention before the sync")]
async fn the_webhook_reported_the_mention(world: &mut ApiWorld) {
    mount_notion(world).await;
    let workspace_id = world.notion_sync.workspace_id.clone().unwrap();
    let blocks = world.notion_sync.page.as_ref().unwrap().blocks.clone();
    let app: &TestedApp = world.app.get().await;
    mock_notion_blocks(&app.notion_mock_server, MY_TOKEN, PAGE_ID, blocks).await;
    let event: NotionWebhookEvent = serde_json::from_value(notion_page_content_event(
        &workspace_id,
        "event-1",
        PAGE_ID,
        &[BLOCK_ID],
        &[ME],
    ))
    .unwrap();
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
    transaction.commit().await.unwrap();
}

#[given(expr = "I have {int} Notion notification(s)")]
#[then(expr = "I have {int} Notion notification(s)")]
async fn i_have_notifications(world: &mut ApiWorld, count: usize) {
    assert_eq!(notion_notifications(world).await.len(), count);
}

#[then("I have no Notion notification")]
async fn i_have_no_notification(world: &mut ApiWorld) {
    assert!(notion_notifications(world).await.is_empty());
}

#[then(expr = "I have {int} Notion notification(s), all {status}")]
async fn i_have_notifications_with_a_status(
    world: &mut ApiWorld,
    count: usize,
    status: UserStatus,
) {
    let notifications = notion_notifications(world).await;
    assert_eq!(notifications.len(), count);
    assert!(
        notifications
            .iter()
            .all(|notification| notification.status == status.0)
    );
}

#[then(expr = "my Notion notification is titled {string}")]
async fn my_notification_is_titled(world: &mut ApiWorld, title: String) {
    assert_eq!(notion_notifications(world).await[0].title, title);
}

#[then(expr = "it is a mention of me in a block of the page \"Roadmap\", saying {string}")]
async fn it_is_a_mention_of_me(world: &mut ApiWorld, text: String) {
    let notification = notion_notifications(world).await.remove(0);
    let ThirdPartyItemData::NotionBlock(block) = &notification.source_item.data else {
        panic!("Expected a NotionBlock item");
    };
    assert_eq!(block.page.id, PAGE_ID);
    assert_eq!(block.user_notion_id.as_deref(), Some(ME));
    assert_eq!(to_plain_text(&block.rich_text), text);
}

#[then(expr = "it holds {int} comments")]
async fn it_holds_comments(world: &mut ApiWorld, count: usize) {
    let notification = notion_notifications(world).await.remove(0);
    let ThirdPartyItemData::NotionComment(discussion) = &notification.source_item.data else {
        panic!("Expected a NotionComment item");
    };
    assert_eq!(discussion.comments.len(), count);
}

#[then("my Notion connection recorded a completed sync")]
async fn my_connection_recorded_a_completed_sync(world: &mut ApiWorld) {
    let id = world.notion_sync.connection.as_ref().unwrap().id;
    let connection = stored_connection(world, id).await;
    assert!(connection.last_notifications_sync_completed_at.is_some());
}

#[then(expr = "Notion was searched {int} time(s)")]
async fn notion_was_searched(world: &mut ApiWorld, times: usize) {
    assert_eq!(requests_to(world, "/v1/search").await, times);
}

#[then(expr = "Notion was asked for the content of {page} {int} time(s)")]
async fn notion_was_asked_for_the_content(world: &mut ApiWorld, page: PageName, times: usize) {
    let reads = requests_to(world, &format!("/v1/blocks/{}", page.0)).await;
    assert_eq!(reads, times);
}

#[then("Notion was asked for the page and its users once each")]
async fn notion_was_asked_for_the_page_and_users_once(world: &mut ApiWorld) {
    assert_eq!(requests_to(world, "/v1/pages/").await, 1);
    assert_eq!(requests_to(world, "/v1/users/").await, 1);
}

#[given("Notion's comments were read")]
#[then("Notion's comments were read")]
async fn the_comments_were_read(world: &mut ApiWorld) {
    assert!(requests_to(world, "/v1/comments").await > 0);
}

#[then("Notion's comments were not read by the last sync")]
async fn the_comments_were_never_read(world: &mut ApiWorld) {
    assert_eq!(requests_to(world, "/v1/comments").await, 0);
}

#[derive(Clone, Copy, Debug)]
pub struct PageName(pub &'static str);

impl Parameter for PageName {
    const REGEX: &'static str = "Roadmap|the older page";
    const NAME: &'static str = "page";
}

impl FromStr for PageName {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "Roadmap" => Ok(Self(PAGE_ID)),
            "the older page" => Ok(Self(OLDER_PAGE_ID)),
            other => Err(format!("Unknown page {other:?}")),
        }
    }
}
