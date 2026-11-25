use chrono::{DateTime, TimeDelta, TimeZone, Timelike, Utc};
use cucumber::{gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;
use uuid::Uuid;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox::{
    PageToken,
    notification::{
        Notification, NotificationId, NotificationListOrder, NotificationSourceKind,
        NotificationStatus, NotificationWithTask,
        service::{NotificationPatch, PatchNotificationsRequest},
    },
    task::{TASK_TITLE_MAX_LENGTH, TaskCreation, TaskPriority},
    third_party::item::ThirdPartyItemKind,
};
use universal_inbox_api::repository::{
    notification::NotificationRepository, third_party::ThirdPartyItemRepository,
};

use crate::{
    helpers::{
        jobs::{count_the_jobs, wait_until_the_jobs_are_done},
        notification::{
            create_task_from_notification_response,
            github::{create_notification_from_github_notification, github_notification},
            linear::{
                create_notification_from_linear_notification, sync_linear_notifications_response,
            },
            list_notifications, list_notifications_page, list_only_snoozed_notifications,
        },
        rest::{
            get_resource, get_resource_response, patch_resource, patch_resource_collection,
            patch_resource_response,
        },
        third_party::overwrite_third_party_item_data,
    },
    world::{ApiWorld, Response},
};

fn parse_statuses(words: &str) -> Vec<NotificationStatus> {
    words
        .split([',', ' '])
        .filter(|word| !word.is_empty() && *word != "and")
        .map(|word| match word {
            "unread" => NotificationStatus::Unread,
            "read" => NotificationStatus::Read,
            "deleted" => NotificationStatus::Deleted,
            "unsubscribed" => NotificationStatus::Unsubscribed,
            other => panic!("Unknown notification status {other:?}"),
        })
        .collect()
}

fn parse_kind(name: &str) -> NotificationSourceKind {
    match name {
        "GitHub" => NotificationSourceKind::Github,
        "Linear" => NotificationSourceKind::Linear,
        "Todoist" => NotificationSourceKind::Todoist,
        other => panic!("Unknown notification source {other:?}"),
    }
}

fn parse_order(order: &str) -> NotificationListOrder {
    match order {
        "ascending" => NotificationListOrder::UpdatedAtAsc,
        "descending" => NotificationListOrder::UpdatedAtDesc,
        other => panic!("Unknown order {other:?}"),
    }
}

pub fn parse_time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn names_in_table(step: &Step) -> Vec<String> {
    step.table
        .as_ref()
        .expect("The notifications must be given as a table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect()
}

async fn create_github_notification(world: &mut ApiWorld, name: &str) -> Notification {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .github_connection
        .as_ref()
        .expect("The account is not connected to GitHub")
        .id;
    let index = world.notifications.created;
    world.notifications.created += 1;
    let mut github = github_notification();
    if index > 0 {
        github.id = (index + 1).to_string();
    }
    let app = world.app.get().await;
    let notification =
        *create_notification_from_github_notification(app, &github, user_id, connection_id).await;
    world
        .notifications
        .by_name
        .insert(name.to_string(), notification.clone());
    notification
}

pub async fn update(world: &mut ApiWorld, name: &str, patch: NotificationPatch) -> Notification {
    let id = world.notifications.named(name).id;
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let status = app
        .repository
        .update_notification(&mut transaction, id, &patch, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let notification = *status.result.unwrap();
    world
        .notifications
        .by_name
        .insert(name.to_string(), notification.clone());
    notification
}

fn now_without_nanoseconds() -> DateTime<Utc> {
    Utc::now().with_nanosecond(0).unwrap()
}

#[given(expr = "I have a GitHub notification called {string}")]
async fn i_have_a_github_notification(world: &mut ApiWorld, name: String) {
    create_github_notification(world, &name).await;
}

#[given(expr = "I have a GitHub notification called {string}, which I marked as read")]
async fn i_have_a_read_github_notification(world: &mut ApiWorld, name: String) {
    create_github_notification(world, &name).await;
    update(
        world,
        &name,
        NotificationPatch {
            status: Some(NotificationStatus::Read),
            ..NotificationPatch::default()
        },
    )
    .await;
}

#[given(expr = "I have a GitHub notification called {string}, which I deleted")]
async fn i_have_a_deleted_github_notification(world: &mut ApiWorld, name: String) {
    create_github_notification(world, &name).await;
    update(
        world,
        &name,
        NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..NotificationPatch::default()
        },
    )
    .await;
}

#[given(
    expr = "I have a GitHub notification called {string}, snoozed until {int} minutes from now"
)]
async fn i_have_a_snoozed_github_notification(world: &mut ApiWorld, name: String, minutes: i64) {
    create_github_notification(world, &name).await;
    update(
        world,
        &name,
        NotificationPatch {
            snoozed_until: Some(
                now_without_nanoseconds() + TimeDelta::try_minutes(minutes).unwrap(),
            ),
            ..NotificationPatch::default()
        },
    )
    .await;
}

#[given(
    expr = "I have a GitHub notification called {string} whose stored item can no longer be decoded"
)]
async fn i_have_a_github_notification_with_a_broken_item(world: &mut ApiWorld, name: String) {
    let notification = create_github_notification(world, &name).await;
    let app = world.app.get().await;
    overwrite_third_party_item_data(
        app,
        notification.source_item.id.0,
        r#"{"type": "GithubNotification", "content": {"unexpected": true}}"#,
    )
    .await;
}

#[given(expr = "I have a Linear notification called {string}")]
async fn i_have_a_linear_notification(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .linear_connection
        .as_ref()
        .expect("The account is not connected to Linear")
        .id;
    let linear_notifications: Vec<
        universal_inbox::third_party::integrations::linear::LinearNotification,
    > = sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    let linear_notification = linear_notifications[2].clone();
    let app = world.app.get().await;
    let notification = *create_notification_from_linear_notification(
        app,
        &linear_notification,
        user_id,
        connection_id,
    )
    .await;
    world.notifications.by_name.insert(name, notification);
}

#[given(expr = "I have {int} GitHub notifications updated in the same second")]
async fn i_have_tied_github_notifications(world: &mut ApiWorld, count: usize) {
    for index in 0..count {
        let notification = create_github_notification(world, &format!("tied {}", index + 1)).await;
        world.notifications.tied.push(notification.id);
    }
    let updated_at = Utc.with_ymd_and_hms(2026, 10, 21, 12, 57, 31).unwrap();
    let user_id = world.user().id;
    let app = world.app.get().await;
    sqlx::query("UPDATE notification SET updated_at = $1 WHERE user_id = $2")
        .bind(updated_at.naive_utc())
        .bind(user_id.0)
        .execute(&*app.repository.pool)
        .await
        .unwrap();
    world.notifications.tied_updated_at = Some(updated_at);
    world.notifications.tied.sort_by_key(|id| id.0);
}

#[when(
    regex = r"^I list my ((?:unread|read|deleted|unsubscribed)(?:(?:, | and )(?:unread|read|deleted|unsubscribed))*) notifications(, snoozed ones included)?(?: of (GitHub|Linear|Todoist))?$"
)]
async fn i_list_my_notifications(
    world: &mut ApiWorld,
    statuses: String,
    snoozed: String,
    kind: String,
) {
    let client = world.client();
    let app = world.app.get().await;
    world.notifications.listed = list_notifications(
        &client,
        &app.api_address,
        parse_statuses(&statuses),
        !snoozed.is_empty(),
        None,
        (!kind.is_empty()).then(|| parse_kind(&kind)),
        false,
    )
    .await;
}

#[when("another user lists their unread notifications")]
async fn another_user_lists_their_notifications(world: &mut ApiWorld) {
    let client = world
        .other_session
        .as_ref()
        .expect("No other user is signed in")
        .client
        .clone();
    let app = world.app.get().await;
    world.notifications.listed = list_notifications(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;
}

#[when(
    regex = r"^I list my snoozed ((?:unread|read|deleted|unsubscribed)(?:(?:, | and )(?:unread|read|deleted|unsubscribed))*) notifications$"
)]
async fn i_list_only_my_snoozed_notifications(world: &mut ApiWorld, statuses: String) {
    let client = world.client();
    let app = world.app.get().await;
    world.notifications.snoozed_listed =
        list_only_snoozed_notifications(&client, &app.api_address, parse_statuses(&statuses)).await;
}

#[then("I see no notifications")]
fn i_see_no_notifications(world: &mut ApiWorld) {
    assert!(world.notifications.listed.is_empty());
}

#[then(expr = "I see only the notification {string}")]
fn i_see_only_the_notification(world: &mut ApiWorld, name: String) {
    assert_eq!(
        world.notifications.listed,
        vec![world.notifications.named(&name).clone()]
    );
}

#[then("I see these notifications, ordered by update time and then by id:")]
fn i_see_these_notifications_in_the_app_order(world: &mut ApiWorld, step: &Step) {
    let mut expected: Vec<Notification> = names_in_table(step)
        .iter()
        .map(|name| world.notifications.named(name).clone())
        .collect();
    expected.sort_by_key(|n| (n.updated_at, n.id.0));
    assert_eq!(world.notifications.listed, expected);
}

#[then(expr = "{string} is among the notifications I see")]
fn a_notification_is_among_those_i_see(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    assert!(world.notifications.listed.iter().any(|n| n.id == id));
}

#[then(expr = "the only snoozed notification I see is {string}")]
fn the_only_snoozed_notification_i_see(world: &mut ApiWorld, name: String) {
    let listed = &world.notifications.snoozed_listed;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, world.notifications.named(&name).id);
}

#[then("I see no snoozed notifications")]
fn i_see_no_snoozed_notifications(world: &mut ApiWorld) {
    assert!(world.notifications.snoozed_listed.is_empty());
}

#[when(expr = "I unsnooze {string}")]
async fn i_unsnooze(world: &mut ApiWorld, name: String) {
    update(
        world,
        &name,
        NotificationPatch {
            snoozed_until: Some(now_without_nanoseconds() - TimeDelta::try_minutes(1).unwrap()),
            ..NotificationPatch::default()
        },
    )
    .await;
}

#[when(expr = "I undelete {string}")]
async fn i_mark_as_unread_again(world: &mut ApiWorld, name: String) {
    update(
        world,
        &name,
        NotificationPatch {
            status: Some(NotificationStatus::Unread),
            ..NotificationPatch::default()
        },
    )
    .await;
}

fn tied_ids_in_order(world: &ApiWorld, order: NotificationListOrder) -> Vec<NotificationId> {
    let mut ids = world.notifications.tied.clone();
    if order == NotificationListOrder::UpdatedAtDesc {
        ids.reverse();
    }
    ids
}

#[when(expr = "I list my unread notifications in {string} order 3 times")]
async fn i_list_my_unread_notifications_in_order(world: &mut ApiWorld, order: String) {
    let order = parse_order(&order);
    let expected = tied_ids_in_order(world, order);
    let client = world.client();
    let app = world.app.get().await;
    for _ in 0..3 {
        let page = list_notifications_page(&client, &app.api_address, order, None).await;
        let listed: Vec<NotificationId> = page.content.iter().map(|n| n.id).collect();
        assert_eq!(listed, expected);
    }
}

#[then(
    expr = "in {string} order, the page after the 2nd of them lists the other 3 and links back to the 3rd"
)]
async fn the_page_after_the_second(world: &mut ApiWorld, order: String) {
    let order = parse_order(&order);
    let ids = tied_ids_in_order(world, order);
    let updated_at = world.notifications.tied_updated_at.unwrap();
    let client = world.client();
    let app = world.app.get().await;
    let page = list_notifications_page(
        &client,
        &app.api_address,
        order,
        Some(PageToken::AfterItem {
            updated_at,
            id: ids[1].0,
        }),
    )
    .await;
    let listed: Vec<NotificationId> = page.content.iter().map(|n| n.id).collect();
    assert_eq!(listed, ids[2..]);
    assert_eq!(
        page.previous_page_token,
        Some(PageToken::BeforeItem {
            updated_at,
            id: ids[2].0,
        })
    );
}

#[then(
    expr = "in {string} order, the page before the 4th of them lists the first 3 and links forward to the 3rd"
)]
async fn the_page_before_the_fourth(world: &mut ApiWorld, order: String) {
    let order = parse_order(&order);
    let ids = tied_ids_in_order(world, order);
    let updated_at = world.notifications.tied_updated_at.unwrap();
    let client = world.client();
    let app = world.app.get().await;
    let page = list_notifications_page(
        &client,
        &app.api_address,
        order,
        Some(PageToken::BeforeItem {
            updated_at,
            id: ids[3].0,
        }),
    )
    .await;
    let listed: Vec<NotificationId> = page.content.iter().map(|n| n.id).collect();
    assert_eq!(listed, ids[..3]);
    assert_eq!(
        page.next_page_token,
        Some(PageToken::AfterItem {
            updated_at,
            id: ids[2].0,
        })
    );
}

#[then(
    expr = "in {string} order, a page token that holds only a time one second before them lists all of them"
)]
async fn a_timestamp_only_page_token(world: &mut ApiWorld, order: String) {
    let order = parse_order(&order);
    let ids = tied_ids_in_order(world, order);
    let updated_at = world.notifications.tied_updated_at.unwrap();
    let cursor = match order {
        NotificationListOrder::UpdatedAtAsc => updated_at - TimeDelta::seconds(1),
        NotificationListOrder::UpdatedAtDesc => updated_at + TimeDelta::seconds(1),
    };
    let client = world.client();
    let app = world.app.get().await;
    let page = list_notifications_page(
        &client,
        &app.api_address,
        order,
        Some(PageToken::After(cursor)),
    )
    .await;
    let listed: Vec<NotificationId> = page.content.iter().map(|n| n.id).collect();
    assert_eq!(listed, ids);
}

#[then(expr = "the stored items of the unread GitHub notifications are only that of {string}")]
async fn the_stored_items_are_only_that_of(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let expected = world.notifications.named(&name).source_item.id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let items = app
        .repository
        .find_third_party_items_with_active_notification_for_user_id(
            &mut transaction,
            ThirdPartyItemKind::GithubNotification,
            NotificationStatus::Unread,
            user_id,
        )
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, expected);
}

#[when(expr = "I get the notification {string}")]
async fn i_get_the_notification(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let response = get_resource_response(&client, &app.api_address, "notifications", id.0).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "that other user gets the notification {string}")]
async fn that_other_user_gets_the_notification(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let response = get_resource_response(&client, &app.api_address, "notifications", id.0).await;
    world.http.record(Response::read(response).await);
}

#[when("I get a notification that does not exist")]
async fn i_get_an_unknown_notification(world: &mut ApiWorld) {
    let id = Uuid::new_v4();
    world.notifications.unknown_id = Some(id);
    let client = world.client();
    let app = world.app.get().await;
    let response = get_resource_response(&client, &app.api_address, "notifications", id).await;
    world.http.record(Response::read(response).await);
}

#[then(expr = "the error message is {string}, with <id> being the id of {string}")]
fn the_error_message_names_a_notification(world: &mut ApiWorld, message: String, name: String) {
    let id = world.notifications.named(&name).id;
    assert_error_body(world, &message.replace("<id>", &id.to_string()));
}

#[then(expr = "the error message is {string}, with <id> being the id of that unknown notification")]
fn the_error_message_names_the_unknown_notification(world: &mut ApiWorld, message: String) {
    let id = world
        .notifications
        .unknown_id
        .expect("No unknown id was used");
    assert_error_body(world, &message.replace("<id>", &id.to_string()));
}

fn assert_error_body(world: &ApiWorld, message: &str) {
    assert_eq!(
        world.http.last().text(),
        json!({ "message": message }).to_string()
    );
}

#[given(expr = "I snooze {string} until {string}")]
#[when(expr = "I snooze {string} until {string}")]
async fn i_snooze_a_notification_until(world: &mut ApiWorld, name: String, time: String) {
    let id = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let patched: Box<Notification> = patch_resource(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &NotificationPatch {
            snoozed_until: Some(parse_time(&time)),
            ..Default::default()
        },
    )
    .await;
    world.notifications.patched = Some(*patched);
}

#[then(expr = "the patched notification is {string} snoozed until {string}")]
fn the_patched_notification_is_snoozed(world: &mut ApiWorld, name: String, time: String) {
    assert_eq!(
        world.notifications.patched.as_ref().unwrap(),
        &Notification {
            snoozed_until: Some(parse_time(&time)),
            ..world.notifications.named(&name).clone()
        }
    );
}

#[when(expr = "I send {string} the status it already has and the snooze time {string}")]
async fn i_send_the_status_it_already_has(world: &mut ApiWorld, name: String, time: String) {
    let notification = world.notifications.named(&name).clone();
    let client = world.client();
    let app = world.app.get().await;
    let response = patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        notification.id.into(),
        &NotificationPatch {
            status: Some(notification.status),
            snoozed_until: Some(parse_time(&time)),
            ..Default::default()
        },
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "that other user marks {string} as deleted")]
async fn that_other_user_deletes(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let response = patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[then(expr = "{string} is still unread")]
async fn a_notification_is_still_unread(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    assert_eq!(notification.status, NotificationStatus::Unread);
}

#[when(expr = "I send {string} a patch with nothing to change")]
async fn i_send_an_empty_patch(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let response = patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &NotificationPatch::default(),
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[when("I mark a notification that does not exist as deleted")]
async fn i_patch_an_unknown_notification(world: &mut ApiWorld) {
    let id = Uuid::new_v4();
    world.notifications.unknown_id = Some(id);
    let client = world.client();
    let app = world.app.get().await;
    let response = patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id,
        &NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[given(expr = "GitHub accepts to delete the notification threads {int} and {int}")]
async fn github_accepts_to_delete_threads(world: &mut ApiWorld, first: u32, second: u32) {
    let app = world.app.get().await;
    for thread in [first, second] {
        Mock::given(method("DELETE"))
            .and(path(format!("/notifications/threads/{thread}")))
            .respond_with(ResponseTemplate::new(205))
            .mount(&app.github_mock_server)
            .await;
    }
}

#[then(expr = "the notifications {string} and {string} are unread")]
fn two_notifications_are_unread(world: &mut ApiWorld, first: String, second: String) {
    for name in [first, second] {
        assert_eq!(
            world.notifications.named(&name).status,
            NotificationStatus::Unread
        );
    }
}

#[then(expr = "the notifications {string} and {string} are not snoozed")]
fn two_notifications_are_not_snoozed(world: &mut ApiWorld, first: String, second: String) {
    for name in [first, second] {
        assert_eq!(world.notifications.named(&name).snoozed_until, None);
    }
}

async fn patch_all_github_notifications(
    world: &mut ApiWorld,
    status: Vec<NotificationStatus>,
    patch: NotificationPatch,
) {
    let client = world.client();
    let app = world.app.get().await;
    world.notifications.bulk_changed = patch_resource_collection(
        &client,
        &app.api_address,
        "notifications",
        &PatchNotificationsRequest {
            status,
            sources: vec![NotificationSourceKind::Github],
            patch,
        },
    )
    .await;
}

#[when("I delete all my unread and read GitHub notifications")]
async fn i_delete_all_my_github_notifications(world: &mut ApiWorld) {
    patch_all_github_notifications(
        world,
        vec![NotificationStatus::Unread, NotificationStatus::Read],
        NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            snoozed_until: None,
            task_id: None,
        },
    )
    .await;
}

#[when(expr = "I snooze all my unread GitHub notifications until {string}")]
async fn i_snooze_all_my_github_notifications(world: &mut ApiWorld, time: String) {
    patch_all_github_notifications(
        world,
        vec![NotificationStatus::Unread],
        NotificationPatch {
            status: None,
            snoozed_until: Some(parse_time(&time)),
            task_id: None,
        },
    )
    .await;
}

#[then(expr = "{int} notifications were changed")]
fn notifications_were_changed(world: &mut ApiWorld, count: usize) {
    assert_eq!(world.notifications.bulk_changed.len(), count);
}

#[then(expr = "all the changed notifications are snoozed until {string} and still unread")]
fn the_changed_notifications_are_snoozed(world: &mut ApiWorld, time: String) {
    let time = parse_time(&time);
    assert!(
        world
            .notifications
            .bulk_changed
            .iter()
            .all(|n| n.snoozed_until == Some(time) && n.status == NotificationStatus::Unread)
    );
}

fn sorted_ids(world: &ApiWorld, step: &Step) -> Vec<NotificationId> {
    let mut ids: Vec<NotificationId> = names_in_table(step)
        .iter()
        .map(|name| world.notifications.named(name).id)
        .collect();
    ids.sort_by_key(|id| id.0);
    ids
}

#[then("I see exactly these notifications, in any order:")]
fn i_see_exactly_these_notifications(world: &mut ApiWorld, step: &Step) {
    let mut listed: Vec<NotificationId> = world.notifications.listed.iter().map(|n| n.id).collect();
    listed.sort_by_key(|id| id.0);
    assert_eq!(listed, sorted_ids(world, step));
}

#[then("the snoozed notifications I see are exactly these, in any order:")]
fn the_snoozed_notifications_i_see(world: &mut ApiWorld, step: &Step) {
    let mut listed: Vec<NotificationId> = world
        .notifications
        .snoozed_listed
        .iter()
        .map(|n| n.id)
        .collect();
    listed.sort_by_key(|id| id.0);
    assert_eq!(listed, sorted_ids(world, step));
}

#[given("I note how many jobs were queued so far")]
async fn i_note_the_jobs_queued(world: &mut ApiWorld) {
    let app = world.app.get().await;
    wait_until_the_jobs_are_done(app).await;
    let pushed = count_the_jobs(app).await.pushed;
    world.notifications.jobs_noted = Some(pushed);
}

#[then(expr = "that change queued {int} jobs, and all of them are done")]
async fn that_change_queued_jobs(world: &mut ApiWorld, count: usize) {
    let noted = world
        .notifications
        .jobs_noted
        .expect("No job count was noted");
    let app = world.app.get().await;
    wait_until_the_jobs_are_done(app).await;
    let counts = count_the_jobs(app).await;
    assert_eq!(counts.pushed - noted, count, "{counts:?}");
    assert_eq!(counts.done, counts.pushed, "{counts:?}");
}

#[when(
    "I create a task from a notification that does not exist, with a title one character too long"
)]
async fn i_create_a_task_with_a_title_too_long(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = create_task_from_notification_response(
        &client,
        &app.api_address,
        Uuid::new_v4().into(),
        Some(TaskCreation {
            title: "a".repeat(TASK_TITLE_MAX_LENGTH as usize + 1),
            body: None,
            project_name: None,
            due_at: None,
            priority: TaskPriority::P4,
            task_provider_kind: None,
            time_config: None,
        }),
    )
    .await;
    world.http.record(Response::read(response).await);
}
