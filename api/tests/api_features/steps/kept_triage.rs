use cucumber::then;
use pretty_assertions::assert_eq;

use universal_inbox::notification::{NotificationStatus, NotificationWithTask};

use crate::{helpers::rest::get_resource, steps::notifications::parse_time, world::ApiWorld};

pub async fn read_notification(world: &mut ApiWorld, name: &str) -> NotificationWithTask {
    let id = world.notifications.named(name).id;
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    *notification
}

#[then(expr = "{string} is read since {string}")]
async fn a_notification_is_read_since(world: &mut ApiWorld, name: String, since: String) {
    let original = world.notifications.named(&name).clone();
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.id, original.id);
    assert_eq!(notification.kind, original.kind);
    assert_eq!(
        notification.source_item.source_id,
        original.source_item.source_id
    );
    assert_eq!(notification.status, NotificationStatus::Read);
    assert_eq!(notification.last_read_at, Some(parse_time(&since)));
}

#[then(expr = "{string} is still snoozed until {string}")]
async fn a_notification_is_still_snoozed(world: &mut ApiWorld, name: String, until: String) {
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.snoozed_until, Some(parse_time(&until)));
}

#[then(expr = "{string} is still linked to my Todoist task")]
async fn a_notification_is_still_linked(world: &mut ApiWorld, name: String) {
    let linked = world.notifications.linked_task.clone().unwrap().id;
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.task.map(|task| task.id), Some(linked));
}
