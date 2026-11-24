use chrono::Utc;
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::linear::{LinearConfig, LinearSyncTaskConfig},
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationWithTask,
    },
    task::{PresetDueDate, ProjectSummary},
    third_party::integrations::linear::LinearNotification,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{create_and_mock_integration_connection, linear_oauth_credential},
        notification::{
            linear::{
                create_notification_from_linear_notification, mock_linear_notifications_query,
                sync_linear_notifications_response,
            },
            sync_notifications,
        },
        rest::get_resource,
    },
    steps::{audit_log::list_events, kept_triage::read_notification, status::UserStatus},
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

#[when("I sync my Linear notifications")]
async fn i_sync_my_linear_notifications(world: &mut ApiWorld) {
    let mut response = sync_linear_notifications_response();
    if let Some(unread_since) = world.linear_tasks.unread_since {
        let node = &mut response.data.as_mut().unwrap().notifications.nodes[2];
        node.read_at = None;
        node.updated_at = unread_since;
    }
    let client = world.client();
    let app = world.app.get().await;
    mock_linear_notifications_query(&app.linear_mock_server, &response).await;
    let _: Vec<Notification> = sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Linear),
        false,
    )
    .await;
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
