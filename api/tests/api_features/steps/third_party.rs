use chrono::Utc;
use cucumber::{then, when};
use pretty_assertions::assert_eq;
use universal_inbox::{
    third_party::{
        integrations::{
            api::{APISource, WEB_PAGE_TITLE_MAX_LENGTH, WebPage},
            todoist::TodoistItem,
        },
        item::ThirdPartyItemData,
    },
    user::UserId,
};
use universal_inbox_api::universal_inbox::UniversalInboxError;

use crate::{
    helpers::{
        TestedApp, rest::create_resource_response, task::todoist::todoist_item,
        third_party::create_task_third_party_item,
    },
    state::StoredItem,
    world::{ApiWorld, Response},
};

fn web_page(url: &str, title: &str, favicon: Option<&str>) -> WebPage {
    WebPage {
        url: url.parse().unwrap(),
        title: title.to_string(),
        timestamp: Utc::now(),
        source: APISource::UniversalInboxExtension,
        favicon: favicon.map(|favicon| favicon.parse().unwrap()),
    }
}

fn todoist_task_data() -> ThirdPartyItemData {
    ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
        project_id: "1111".to_string(),
        ..*todoist_item()
    }))
}

async fn try_create_task_item(
    app: &TestedApp,
    data: ThirdPartyItemData,
    user_id: UserId,
) -> Result<(), UniversalInboxError> {
    let mut transaction = app.repository.begin().await.unwrap();
    app.third_party_item_service
        .read()
        .await
        .create_task_item(&mut transaction, data, user_id)
        .await
        .map(|_| ())
}

#[when("a Todoist task is stored for me")]
async fn a_todoist_task_is_stored_for_me(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let data = todoist_task_data();
    let app = world.app.get().await;
    let created = create_task_third_party_item(app, data.clone(), user_id).await;
    world.third_party.created = Some(StoredItem {
        item: created.third_party_item,
        data,
        task: created.task,
    });
}

#[when("I try to store a Todoist task for myself")]
async fn i_try_to_store_a_todoist_task(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let result = try_create_task_item(app, todoist_task_data(), user_id).await;
    world.third_party.refusal = result.err();
}

#[when("I try to store a web page as a task for myself")]
async fn i_try_to_store_a_web_page_as_a_task(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let data =
        ThirdPartyItemData::WebPage(Box::new(web_page("https://example.com", "example", None)));
    let app = world.app.get().await;
    let result = try_create_task_item(app, data, user_id).await;
    world.third_party.refusal = result.err();
}

#[then(
    "the stored item is mine, comes from my Todoist connection and holds the data that was given, and a task was created"
)]
fn the_stored_item_is_mine(world: &mut ApiWorld) {
    let created = world.third_party.created.as_ref().unwrap();
    assert_eq!(created.item.user_id, world.user().id);
    assert_eq!(
        created.item.integration_connection_id,
        world.integrations.todoist_connection.as_ref().unwrap().id
    );
    assert_eq!(created.item.data, created.data);
    assert!(created.task.is_some());
}

#[then(expr = "the store is refused as an unsupported action saying {string}")]
fn the_store_is_refused(world: &mut ApiWorld, message: String) {
    let refusal = &world.third_party.refusal;
    assert!(
        matches!(
            refusal,
            Some(UniversalInboxError::UnsupportedAction(text)) if text.contains(&message)
        ),
        "expected an unsupported action saying {message:?}, got: {refusal:?}"
    );
}

#[when(expr = "I send a Todoist item to {string}")]
async fn i_send_a_todoist_item(world: &mut ApiWorld, path: String) {
    let client = world.client();
    let app = world.app.get().await;
    let response = create_resource_response(
        &client,
        &app.api_address,
        &path,
        Box::new(ThirdPartyItemData::TodoistItem(todoist_item())),
    )
    .await;
    world.http.record(Response::read(response).await);
}

async fn send_web_page(world: &mut ApiWorld, page: WebPage) {
    let client = world.client();
    let app = world.app.get().await;
    let response = create_resource_response(
        &client,
        &app.api_address,
        "third_party/notification/items",
        Box::new(ThirdPartyItemData::WebPage(Box::new(page))),
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "I send a web page with the url {string} to the notification items")]
async fn i_send_a_web_page_with_a_url(world: &mut ApiWorld, url: String) {
    send_web_page(world, web_page(&url, "example", None)).await;
}

#[when(expr = "I send a web page with the favicon {string} to the notification items")]
async fn i_send_a_web_page_with_a_favicon(world: &mut ApiWorld, favicon: String) {
    send_web_page(
        world,
        web_page("https://example.com", "example", Some(&favicon)),
    )
    .await;
}

#[when("I send a web page whose title is one character too long to the notification items")]
async fn i_send_a_web_page_with_a_long_title(world: &mut ApiWorld) {
    let title = "a".repeat(WEB_PAGE_TITLE_MAX_LENGTH + 1);
    send_web_page(world, web_page("https://example.com", &title, None)).await;
}
