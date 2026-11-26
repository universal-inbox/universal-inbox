use chrono::Utc;
use cucumber::{then, when};
use pretty_assertions::assert_eq;

use universal_inbox::{
    HasHtmlUrl,
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationWithTask,
    },
    third_party::{
        integrations::api::{APISource, WebPage},
        item::{ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use crate::{
    helpers::rest::{create_resource_response, get_resource, get_resource_response},
    world::{ApiWorld, Response},
};

async fn send(world: &mut ApiWorld, client: reqwest::Client, url: &str, title: &str) -> WebPage {
    let page = WebPage {
        url: url.parse().unwrap(),
        title: title.to_string(),
        timestamp: Utc::now(),
        source: APISource::UniversalInboxExtension,
        favicon: None,
    };
    let app = world.app.get().await;
    let response = create_resource_response(
        &client,
        &app.api_address,
        "third_party/notification/items",
        Box::new(ThirdPartyItemData::WebPage(Box::new(page.clone()))),
    )
    .await;
    world.http.record(Response::read(response).await);
    page
}

fn the_answered_notification(world: &ApiWorld, page: &WebPage) -> Notification {
    assert_eq!(world.http.last().status, 200);
    let creation: ThirdPartyItemCreationResult = world.http.last().json();
    assert!(creation.task.is_none());
    assert_eq!(
        creation.third_party_item.data,
        ThirdPartyItemData::WebPage(Box::new(page.clone()))
    );
    let notification = creation
        .notification
        .expect("The answer carries no notification");
    assert_eq!(notification.source_item, creation.third_party_item);
    notification
}

#[when(
    expr = "I send the web page {string}, titled {string}, to the notification items, as {string}"
)]
async fn i_send_a_web_page(world: &mut ApiWorld, url: String, title: String, name: String) {
    let client = world.client();
    let page = send(world, client, &url, &title).await;
    let notification = the_answered_notification(world, &page);
    world.notifications.by_name.insert(name, notification);
}

#[when(
    expr = "that other user sends the web page {string}, titled {string}, to the notification items, as {string}"
)]
async fn the_other_user_sends_a_web_page(
    world: &mut ApiWorld,
    url: String,
    title: String,
    name: String,
) {
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let page = send(world, client, &url, &title).await;
    let notification = the_answered_notification(world, &page);
    world.notifications.by_name.insert(name, notification);
}

#[then(
    expr = "{string} is an unread notification of mine, from the API, titled {string} and opening {string}"
)]
fn the_notification_is(world: &mut ApiWorld, name: String, title: String, url: String) {
    let user_id = world.user().id;
    let notification = world.notifications.named(&name);
    assert_eq!(
        (
            notification.user_id,
            notification.status,
            notification.kind,
            notification.title.as_str(),
            notification.get_html_url().to_string()
        ),
        (
            user_id,
            NotificationStatus::Unread,
            NotificationSourceKind::API,
            title.as_str(),
            url.parse::<url::Url>().unwrap().to_string()
        )
    );
}

#[then(expr = "{string}, read again, is the notification that was answered")]
async fn the_notification_is_stored(world: &mut ApiWorld, name: String) {
    let answered = world.notifications.named(&name).clone();
    let client = world.client();
    let app = world.app.get().await;
    let stored: Box<NotificationWithTask> = get_resource(
        &client,
        &app.api_address,
        "notifications",
        answered.id.into(),
    )
    .await;
    assert_eq!(Notification::from(*stored), answered);
}

#[then(expr = "{string} and {string} are two notifications, of two items")]
fn they_are_two_notifications(world: &mut ApiWorld, first: String, second: String) {
    let first = world.notifications.named(&first);
    let second = world.notifications.named(&second);
    assert_ne!(first.id, second.id);
    assert_ne!(first.source_item.id, second.source_item.id);
}

#[then(expr = "{string} belongs to that other user")]
fn it_belongs_to_the_other_user(world: &mut ApiWorld, name: String) {
    let other_user_id = world.other_session.as_ref().unwrap().user.id;
    assert_eq!(world.notifications.named(&name).user_id, other_user_id);
}

#[when(expr = "that other user tries to read {string}")]
async fn the_other_user_tries_to_read(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let response = get_resource_response(&client, &app.api_address, "notifications", id.0).await;
    world.http.record(Response::read(response).await);
}
