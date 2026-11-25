use cucumber::{given, when};

use universal_inbox::notification::{NotificationStatus, service::NotificationPatch};

use crate::{
    helpers::{
        notification::todoist::create_notification_from_todoist_item,
        rest::patch_resource_response, task::todoist::todoist_item,
    },
    world::{ApiWorld, Response},
};

#[given(expr = "the notification of that task is named {string}")]
fn the_notification_of_the_task_is_named(world: &mut ApiWorld, name: String) {
    let notification = world
        .todoist_task_changes
        .items
        .last()
        .expect("I have no Todoist task")
        .1
        .notification
        .clone()
        .expect("That Todoist task has no notification");
    world.notifications.by_name.insert(name, notification);
}

#[given(expr = "I have the notification {string} of a Todoist item")]
async fn i_have_the_notification_of_a_todoist_item(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .todoist_connection
        .as_ref()
        .expect("The account is not connected to Todoist")
        .id;
    let app = world.app.get().await;
    let notification =
        create_notification_from_todoist_item(app, &todoist_item(), user_id, connection_id).await;
    world.notifications.by_name.insert(name, *notification);
}

#[when(expr = "I try to unsubscribe from {string}")]
async fn i_try_to_unsubscribe(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let response = patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &NotificationPatch {
            status: Some(NotificationStatus::Unsubscribed),
            ..Default::default()
        },
    )
    .await;
    world.http.record(Response::read(response).await);
}
