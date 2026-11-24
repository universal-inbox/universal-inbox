use chrono::Utc;
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::linear::{LinearConfig, LinearSyncTaskConfig},
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationSyncSourceKind,
        NotificationWithTask, service::NotificationPatch,
    },
    task::{PresetDueDate, ProjectSummary, TaskStatus},
    third_party::{integrations::linear::LinearNotification, item::ThirdPartyItemData},
};
use universal_inbox_api::{
    integrations::linear::graphql::notifications_query::{
        NotificationsQueryNotificationsNodesOn,
        NotificationsQueryNotificationsNodesOnIssueNotification,
        NotificationsQueryNotificationsNodesOnIssueNotificationComment,
    },
    repository::notification::NotificationRepository,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{create_and_mock_integration_connection, linear_oauth_credential},
        notification::{
            linear::{
                assert_sync_notifications, create_notification_from_linear_notification,
                mock_linear_notifications_query, sync_linear_notifications_response,
            },
            list_notifications, set_notification_updated_at, sync_notifications,
        },
        rest::get_resource,
        task::list_tasks,
    },
    steps::{
        audit_log::list_events,
        integration_connections::stored_connection,
        kept_triage::read_notification,
        notifications::{parse_time, update},
        status::UserStatus,
    },
    world::ApiWorld,
};

#[given(
    expr = "I received the Linear notification of that issue, {string}, which was created already deleted"
)]
async fn i_received_the_notification_of_that_issue(world: &mut ApiWorld, name: String) {
    let notification = create_the_notification(world).await;
    assert_eq!(notification.status, NotificationStatus::Deleted);
    world.notifications.by_name.insert(name, *notification);
}

#[given(expr = "I already have the Linear notification of that issue, {string}, which I have read")]
async fn i_already_have_the_notification(world: &mut ApiWorld, name: String) {
    let notification = create_the_notification(world).await;
    assert_eq!(notification.status, NotificationStatus::Read);
    world.notifications.by_name.insert(name, *notification);
}

async fn create_the_notification(world: &mut ApiWorld) -> Box<Notification> {
    let user_id = world.user().id;
    let linear_id = world.integrations.linear_connection.as_ref().unwrap().id;
    let notifications: Vec<LinearNotification> = sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    let app = world.app.get().await;
    create_notification_from_linear_notification(app, &notifications[2], user_id, linear_id).await
}

#[given("Linear reports new unread activity on that notification")]
fn linear_reports_new_activity(world: &mut ApiWorld) {
    world.linear_tasks.unread_since = Some(Utc::now());
}

#[then(expr = "my Linear notification {string} is {status}")]
async fn my_notification_is(world: &mut ApiWorld, name: String, status: UserStatus) {
    let id = world.notifications.named(&name).id;
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.id, id);
    assert_eq!(notification.status, status.0);
}

async fn status_changes(world: &mut ApiWorld, name: &str) -> usize {
    let id = world.notifications.named(name).id;
    let client = world.client();
    let app = world.app.get().await;
    list_events(&client, app, &format!("entity_id={id}&change=Status"))
        .await
        .content
        .len()
}

#[then(expr = "the status of {string} never changed")]
async fn the_status_never_changed(world: &mut ApiWorld, name: String) {
    assert_eq!(status_changes(world, &name).await, 0);
}

#[given(
    "that other user is connected to Linear, syncing its tasks into the project Project2, due today"
)]
async fn that_other_user_is_connected_to_linear(world: &mut ApiWorld) {
    let user_id = world.other_session.as_ref().unwrap().user.id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(ProjectSummary {
                    name: "Project2".to_string(),
                    source_id: "2222".into(),
                }),
                default_due_at: Some(PresetDueDate::Today),
                ..Default::default()
            },
        }),
        &settings(),
        linear_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.other_linear_connection = Some(connection);
}

#[given(
    expr = "that other user has the Linear notification of that issue, {string}, which they have read"
)]
async fn that_other_user_has_the_notification(world: &mut ApiWorld, name: String) {
    let user_id = world.other_session.as_ref().unwrap().user.id;
    let connection_id = world
        .integrations
        .other_linear_connection
        .as_ref()
        .unwrap()
        .id;
    let notifications: Vec<LinearNotification> = sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    let app = world.app.get().await;
    let notification = create_notification_from_linear_notification(
        app,
        &notifications[2],
        user_id,
        connection_id,
    )
    .await;
    assert_eq!(notification.status, NotificationStatus::Read);
    world.notifications.by_name.insert(name, *notification);
}

#[then(expr = "the Linear notification {string} of that other user is still {status}")]
async fn the_other_users_notification_is(world: &mut ApiWorld, name: String, status: UserStatus) {
    let id = world.notifications.named(&name).id;
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    assert_eq!(notification.status, status.0);
}

#[when(expr = "the Linear notification of that issue arrives as {string}")]
async fn the_notification_arrives(world: &mut ApiWorld, name: String) {
    let notification = create_the_notification(world).await;
    world.notifications.by_name.insert(name, *notification);
}

#[then("that other user has no task")]
async fn that_other_user_has_no_task(world: &mut ApiWorld) {
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let tasks = list_tasks(&client, &app.api_address, TaskStatus::Active, false).await;
    assert!(tasks.is_empty());
}

#[when("I sync my Linear notifications")]
async fn i_sync_my_linear_notifications(world: &mut ApiWorld) {
    let mut response = sync_linear_notifications_response();
    let data = response.data.as_mut().unwrap();
    if let Some(unread_since) = world.linear_tasks.unread_since {
        let node = &mut data.notifications.nodes[2];
        node.read_at = None;
        node.updated_at = unread_since;
    }
    if let Some(second_id) = world.linear_tasks.second_notification {
        let mut node = data.notifications.nodes[2].clone();
        node.id = second_id.to_string();
        node.updated_at = Utc::now();
        if let NotificationsQueryNotificationsNodesOn::IssueNotification(
            NotificationsQueryNotificationsNodesOnIssueNotification { comment, .. },
        ) = &mut node.on
        {
            *comment = Some(
                NotificationsQueryNotificationsNodesOnIssueNotificationComment {
                    body: "Updated comment".to_string(),
                    updated_at: Utc::now(),
                    user: None,
                    url: "https://test.com".to_string(),
                    parent: None,
                },
            );
        }
        data.notifications.nodes.push(node);
    }
    let client = world.client();
    let app = world.app.get().await;
    mock_linear_notifications_query(&app.linear_mock_server, &response).await;
    world.linear_tasks.synced = sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Linear),
        false,
    )
    .await;
}

#[given("Linear reports a second notification of that issue, with a newer comment")]
fn linear_reports_a_second_notification(world: &mut ApiWorld) {
    world.linear_tasks.second_notification = Some(uuid::Uuid::new_v4());
}

#[when(expr = "I take note of the unread notification Linear reported as {string}")]
fn i_take_note_of_the_unread_notification(world: &mut ApiWorld, name: String) {
    let notification = world
        .linear_tasks
        .synced
        .iter()
        .find(|notification| notification.status == NotificationStatus::Unread)
        .expect("the Linear fixture must have an unread notification")
        .clone();
    world.notifications.by_name.insert(name, notification);
}

#[when(expr = "I read {string} in Universal Inbox only, which was last updated at {string}")]
async fn i_read_it_in_universal_inbox_only(world: &mut ApiWorld, name: String, updated_at: String) {
    let updated_at = parse_time(&updated_at);
    update(
        world,
        &name,
        NotificationPatch {
            status: Some(NotificationStatus::Read),
            ..Default::default()
        },
    )
    .await;
    let id = world.notifications.named(&name).id;
    let app = world.app.get().await;
    set_notification_updated_at(app, id, updated_at).await;
}

#[when("I force a new sync of my Linear notifications, which reports the same notifications")]
async fn i_force_a_new_sync(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    app.notification_service
        .read()
        .await
        .sync_notifications_with_transaction(NotificationSyncSourceKind::Linear, user_id, true)
        .await
        .unwrap();
}

#[then(expr = "{string} is still read, and was last updated at {string}")]
async fn it_is_still_read_and_untouched(world: &mut ApiWorld, name: String, updated_at: String) {
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.status, NotificationStatus::Read);
    assert_eq!(notification.updated_at, parse_time(&updated_at));
}

#[then("my notifications are the ones Linear reported")]
fn my_notifications_are_the_reported_ones(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let reported: Vec<LinearNotification> = sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(world.linear_tasks.synced.len(), reported.len());
    assert_sync_notifications(&world.linear_tasks.synced, &reported, user_id);
}

#[then(
    expr = "{string} now follows what Linear reported: read since {string}, snoozed until {string}"
)]
async fn it_now_follows_linear(
    world: &mut ApiWorld,
    name: String,
    read_since: String,
    snoozed_until: String,
) {
    let original = world.notifications.named(&name).clone();
    let reported: Vec<LinearNotification> = sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.id, original.id);
    assert_eq!(notification.kind, NotificationSourceKind::Linear);
    assert_eq!(
        notification.source_item.source_id,
        original.source_item.source_id
    );
    assert_eq!(notification.status, NotificationStatus::Read);
    assert_eq!(notification.last_read_at, Some(parse_time(&read_since)));
    assert_eq!(notification.snoozed_until, Some(parse_time(&snoozed_until)));
    assert_eq!(
        notification.source_item.data,
        ThirdPartyItemData::LinearNotification(Box::new(reported[2].clone()))
    );
}

#[then("Linear reported comments parsed with only their parent comment and its answers")]
fn the_comments_were_parsed(_world: &mut ApiWorld) {
    let reported: Vec<LinearNotification> = sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    match &reported[2] {
        LinearNotification::IssueNotification {
            comment: Some(comment),
            ..
        } => {
            assert_eq!(comment.body, "Initial comment".to_string());
            assert_eq!(comment.children.len(), 2);
            assert_eq!(comment.children[0].body, "other comment".to_string());
            assert_eq!(comment.children[1].body, "answer comment".to_string());
        }
        _ => unreachable!("Expected Linear issue notification"),
    }
    match &reported[3] {
        LinearNotification::IssueNotification {
            comment: Some(comment),
            ..
        } => {
            assert_eq!(comment.body, "comment without parent".to_string());
            assert!(comment.children.is_empty());
        }
        _ => unreachable!("Expected Linear issue notification"),
    }
}

#[then(
    expr = "the sync returned {int} Linear notifications, as two of them are about the same issue"
)]
fn the_sync_returned_notifications(world: &mut ApiWorld, count: usize) {
    assert_eq!(world.linear_tasks.synced.len(), count);
}

#[then("the newer notification of that issue is read")]
async fn the_newer_notification_is_read(world: &mut ApiWorld) {
    let second_id = world.linear_tasks.second_notification.unwrap().to_string();
    let client = world.client();
    let app = world.app.get().await;
    let notifications =
        list_notifications(&client, &app.api_address, vec![], false, None, None, false).await;
    let newer = notifications
        .iter()
        .find(|notification| notification.source_item.source_id == second_id)
        .expect("The newer notification was not synced");
    assert_eq!(newer.status, NotificationStatus::Read);
    let issue_id = match &reported_notifications()[2] {
        LinearNotification::IssueNotification { issue, .. } => issue.id,
        _ => unreachable!("Expected Linear issue notification"),
    };
    let of_that_issue = notifications
        .iter()
        .filter(|notification| {
            matches!(
                &notification.source_item.data,
                ThirdPartyItemData::LinearNotification(linear)
                    if matches!(&**linear, LinearNotification::IssueNotification { issue, .. } if issue.id == issue_id)
            )
        })
        .count();
    assert_eq!(of_that_issue, 2);
}

#[then("my Linear connection recorded a completed notifications sync, without failure")]
async fn the_connection_recorded_a_completed_sync(world: &mut ApiWorld) {
    let id = world.integrations.linear_connection.as_ref().unwrap().id;
    let connection = stored_connection(world, id).await;
    assert!(connection.last_notifications_sync_started_at.is_some());
    assert!(connection.last_notifications_sync_completed_at.is_some());
    assert!(connection.last_notifications_sync_failed_at.is_none());
    assert!(connection.last_notifications_sync_failure_message.is_none());
    assert_eq!(connection.notifications_sync_failures, 0);
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert!(connection.failure_message.is_none());
}

#[given(
    expr = "the snooze of {string} until {string} is stored directly, as Linear does not allow it from the API"
)]
async fn the_snooze_is_stored_directly(world: &mut ApiWorld, name: String, until: String) {
    let id = world.notifications.named(&name).id;
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_notification(
            &mut transaction,
            id,
            &NotificationPatch {
                snoozed_until: Some(parse_time(&until)),
                ..Default::default()
            },
            user_id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.snoozed_until, Some(parse_time(&until)));
}

fn reported_notifications() -> Vec<LinearNotification> {
    sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap()
}
