use std::collections::HashSet;

use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    notification::NotificationSourceKind,
    realtime::{RealtimeEvent, RealtimeEventKind},
    slack_bridge::SlackBridgeActionStatus,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{create_and_mock_integration_connection, slack_oauth_credential},
        notification::slack::{create_notification_from_slack_thread, slack_thread},
        realtime::{RealtimeEventsSubscriber, events_of_kind},
        slack_bridge::pending_action_ids,
    },
    steps::slack_bridge::seed_action,
    world::ApiWorld,
};

#[given("I listen to the realtime events of my user")]
async fn i_listen_to_the_realtime_events(world: &mut ApiWorld) {
    let user_id = world.user().id;
    world.realtime_publication.subscriber =
        Some(RealtimeEventsSubscriber::new(&settings(), user_id).await);
}

async fn collect_until(
    world: &mut ApiWorld,
    kind: RealtimeEventKind,
    count: usize,
) -> Vec<RealtimeEvent> {
    let events = world
        .realtime_publication
        .subscriber
        .as_mut()
        .expect("The scenario does not listen to the realtime events")
        .events_until(|events| events_of_kind(events, kind).len() >= count)
        .await;
    world.realtime_publication.events = events.clone();
    events
}

fn user_id_of(world: &ApiWorld) -> universal_inbox::user::UserId {
    world.user().id
}

#[then(expr = "I received exactly one notifications.changed event, about {string} only")]
async fn i_received_one_event_about(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let events = collect_until(world, RealtimeEventKind::NotificationsChanged, 1).await;
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::NotificationsChanged),
        vec![RealtimeEvent {
            user_id: user_id_of(world),
            kind: RealtimeEventKind::NotificationsChanged,
            ids: Some(vec![id.0]),
        }]
    );
}

#[then("I received one notifications.changed event, about all the synced notifications")]
async fn i_received_the_batched_event(world: &mut ApiWorld) {
    let events = world
        .realtime_publication
        .subscriber
        .as_mut()
        .expect("The scenario does not listen to the realtime events")
        .events_until(|events| {
            events_of_kind(events, RealtimeEventKind::IntegrationConnectionChanged).len() >= 2
                && !events_of_kind(events, RealtimeEventKind::NotificationsChanged).is_empty()
        })
        .await;
    world.realtime_publication.events = events.clone();
    let received = events_of_kind(&events, RealtimeEventKind::NotificationsChanged);
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].user_id, user_id_of(world));
    let received_ids: HashSet<Uuid> = received[0].ids.clone().unwrap().into_iter().collect();
    let synced_ids: HashSet<Uuid> = world
        .github_sync
        .synced
        .iter()
        .map(|notification| notification.id.0)
        .collect();
    assert_eq!(received_ids, synced_ids);
}

#[then("I received two integration_connection.changed events, about my GitHub connection only")]
async fn i_received_the_connection_events(world: &mut ApiWorld) {
    let events = world.realtime_publication.events.clone();
    let id = world
        .integrations
        .github_connection
        .as_ref()
        .expect("The account is not connected to GitHub")
        .id;
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::IntegrationConnectionChanged),
        vec![
            RealtimeEvent {
                user_id: user_id_of(world),
                kind: RealtimeEventKind::IntegrationConnectionChanged,
                ids: Some(vec![id.0]),
            };
            2
        ]
    );
}

#[given("I am connected to Slack, syncing its notifications, with the extension bridge enabled")]
async fn i_am_connected_with_the_bridge(world: &mut ApiWorld) {
    let mut config = SlackConfig::enabled_as_notifications();
    config.message_config.extension_enabled = true;
    connect_to_slack(world, config).await;
}

#[given("I am connected to Slack, syncing its notifications, with the extension bridge disabled")]
async fn i_am_connected_without_the_bridge(world: &mut ApiWorld) {
    let mut config = SlackConfig::enabled_as_notifications();
    config.message_config.extension_enabled = false;
    connect_to_slack(world, config).await;
}

async fn connect_to_slack(world: &mut ApiWorld, config: SlackConfig) {
    let user_id = user_id_of(world);
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(config),
        &settings(),
        slack_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.slack_connection = Some(connection);
}

#[given(expr = "I have the Slack notification {string}")]
async fn i_have_the_slack_notification(world: &mut ApiWorld, name: String) {
    let user_id = user_id_of(world);
    let connection_id = world
        .integrations
        .slack_connection
        .as_ref()
        .expect("The account is not connected to Slack")
        .id;
    let app = world.app.get().await;
    let notification =
        create_notification_from_slack_thread(app, &slack_thread(), user_id, connection_id).await;
    assert_eq!(notification.kind, NotificationSourceKind::Slack);
    world.notifications.by_name.insert(name, *notification);
}

#[given(expr = "I have two Pending Slack bridge actions that no event announced")]
async fn i_have_two_actions(world: &mut ApiWorld) {
    for _ in 0..2 {
        seed_action(world, SlackBridgeActionStatus::Pending).await;
        let id = world
            .slack_bridge
            .action_id
            .expect("The action was not seeded");
        world.realtime_publication.seeded_actions.push(id.0);
    }
}

#[when("the Slack extension reports the first of them as completed and the second as failed")]
async fn the_extension_reports_both(world: &mut ApiWorld) {
    let ids = world.realtime_publication.seeded_actions.clone();
    let client = world.client();
    let app = world.app.get().await;
    for (id, verb) in [(ids[0], "complete"), (ids[1], "fail")] {
        let response = client
            .post(format!(
                "{}slack-bridge/actions/{id}/{verb}",
                app.api_address
            ))
            .json(&serde_json::json!({ "error": "failed" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
}

#[then(
    "I received exactly one slack_bridge_pending_actions.changed event, about the action it created"
)]
async fn i_received_the_pending_actions_event(world: &mut ApiWorld) {
    let events = collect_until(
        world,
        RealtimeEventKind::SlackBridgePendingActionsChanged,
        1,
    )
    .await;
    let user_id = user_id_of(world);
    let seeded = world.realtime_publication.seeded_actions.clone();
    let app = world.app.get().await;
    let created: Vec<Uuid> = pending_action_ids(app, user_id)
        .await
        .into_iter()
        .filter(|id| !seeded.contains(id))
        .collect();
    assert_eq!(created.len(), 1);
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::SlackBridgePendingActionsChanged),
        vec![RealtimeEvent {
            user_id,
            kind: RealtimeEventKind::SlackBridgePendingActionsChanged,
            ids: Some(created),
        }]
    );
}

#[then(
    "the deletion has finished, as its own notifications.changed event arrived, and no slack_bridge_pending_actions.changed event came with it, nor any action"
)]
async fn i_received_no_pending_actions_event(world: &mut ApiWorld) {
    let events = collect_until(world, RealtimeEventKind::NotificationsChanged, 1).await;
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::SlackBridgePendingActionsChanged),
        vec![]
    );
    let user_id = user_id_of(world);
    let app = world.app.get().await;
    assert_eq!(pending_action_ids(app, user_id).await, Vec::<Uuid>::new());
}

#[when(expr = "I delete the Slack notification {string}")]
async fn i_delete_the_slack_notification(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let response = crate::helpers::rest::patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &universal_inbox::notification::service::NotificationPatch {
            status: Some(universal_inbox::notification::NotificationStatus::Deleted),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(response.status(), 200);
}

#[given("that other user listens to the realtime events of their user")]
async fn that_other_user_listens(world: &mut ApiWorld) {
    let other_user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    world.realtime_publication.other_subscriber =
        Some(RealtimeEventsSubscriber::new(&settings(), other_user_id).await);
}

#[when(expr = "that other user snoozes {string} until {string}")]
async fn that_other_user_snoozes(world: &mut ApiWorld, name: String, time: String) {
    let id = world.notifications.named(&name).id;
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let response = crate::helpers::rest::patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &universal_inbox::notification::service::NotificationPatch {
            snoozed_until: Some(crate::steps::notifications::parse_time(&time)),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(response.status(), 200);
}

#[then(
    expr = "that other user received exactly one notifications.changed event, about {string} only, and none about mine"
)]
async fn that_other_user_received_one_event(world: &mut ApiWorld, name: String) {
    let their_id = world.notifications.named(&name).id;
    let other_user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    let events = world
        .realtime_publication
        .other_subscriber
        .as_mut()
        .expect("That other user does not listen to the realtime events")
        .events_until(|events| {
            !events_of_kind(events, RealtimeEventKind::NotificationsChanged).is_empty()
        })
        .await;
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::NotificationsChanged),
        vec![RealtimeEvent {
            user_id: other_user_id,
            kind: RealtimeEventKind::NotificationsChanged,
            ids: Some(vec![their_id.0]),
        }]
    );
}
