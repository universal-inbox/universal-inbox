use cucumber::then;
use pretty_assertions::assert_eq;

use universal_inbox::notification::NotificationStatus;

use crate::{helpers::notification::list_notifications, world::ApiWorld};

#[then(expr = "my only unread notification is {string}")]
async fn my_only_unread_notification_is(world: &mut ApiWorld, name: String) {
    let expected = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;
    assert_eq!(
        notifications.iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![expected]
    );
}
