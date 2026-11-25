use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path, query_param},
};

use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use slack_blocks_render::SlackReferences;
use slack_morphism::prelude::*;

use universal_inbox_api::integrations::todoist::TodoistSyncCommandItemUncompleteArgs;

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackReactionConfig, SlackSyncType},
    },
    notification::{NotificationSourceKind, NotificationStatus},
    task::TaskStatus,
    third_party::{
        integrations::{
            slack::{SlackMessageSenderDetails, SlackReactionItem, SlackReactionState},
            todoist::{TodoistItem, TodoistItemPriority},
        },
        item::{ThirdPartyItemData, ThirdPartyItemSource, ThirdPartyItemSourceKind},
    },
};

use crate::{
    common::settings,
    helpers::{
        fixture_path,
        integration_connection::{
            create_and_mock_integration_connection, slack_context, slack_oauth_credential,
        },
        jobs::wait_until_the_jobs_are_done,
        notification::{
            list_notifications, list_notifications_until, slack::slack_push_reaction_added_event,
            slack::slack_push_reaction_removed_event,
        },
        slack::post_signed_slack_event,
        task::{
            list_synced_tasks, list_tasks_until,
            todoist::{
                TodoistSyncPartialCommand, mock_todoist_get_item_service,
                mock_todoist_item_add_service_expecting_one_call, todoist_item,
            },
        },
    },
    steps::slack_tasks::{
        DESCRIPTION, ReactionEvent, TITLE_OF_A_SLACK_LINK, connect_to_slack,
        mount_the_message_context,
    },
    steps::todoist_task_changes::expect_once,
    world::ApiWorld,
};

const MESSAGE_TS: &str = "1707686216.825719";
const SLOW_ANSWER: Duration = Duration::from_millis(1500);

#[given("I am connected to Slack, with its sync disabled")]
async fn i_am_connected_to_slack_with_its_sync_disabled(world: &mut ApiWorld) {
    connect_to_slack(world, SlackConfig::disabled()).await;
}

#[given(expr = "I am connected to Slack, syncing the reaction {string} as notifications")]
async fn i_am_connected_to_slack_for_another_reaction(world: &mut ApiWorld, name: String) {
    connect_to_slack(
        world,
        SlackConfig {
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                reaction_name: SlackReactionName(name),
                sync_type: SlackSyncType::AsNotifications,
                completion_reaction_name: None,
            },
            ..SlackConfig::default()
        },
    )
    .await;
}

#[given("Todoist adds the Slack message as an item of the Inbox, and then serves its item")]
async fn todoist_adds_the_message_in_the_inbox(world: &mut ApiWorld) {
    let item: TodoistItem = *todoist_item();
    let app = world.app.get().await;
    mock_todoist_item_add_service_expecting_one_call(
        &app.todoist_mock_server,
        &item.id,
        TITLE_OF_A_SLACK_LINK.to_string(),
        Some(DESCRIPTION.to_string()),
        Some("1111".to_string()),
        None,
        TodoistItemPriority::P1,
    )
    .await;
    mock_todoist_get_item_service(&app.todoist_mock_server, Box::new(item)).await;
}

#[given(expr = "Slack delivers the event of the reaction \"eyes\" being {reaction_event}")]
#[when(expr = "Slack delivers the event of the reaction \"eyes\" being {reaction_event}")]
async fn slack_delivers_the_reaction_event(world: &mut ApiWorld, event: ReactionEvent) {
    let client = world.client();
    let app = world.app.get().await;
    let pushed = match event {
        ReactionEvent::Added => slack_push_reaction_added_event(),
        ReactionEvent::Removed => slack_push_reaction_removed_event(),
    };
    world.slack_tasks.delivered_at = Some(Instant::now());
    let response = post_signed_slack_event(&client, &app.api_address, &*pushed).await;
    assert_eq!(response.status(), 200);
}

#[then(
    "I get one unread notification, which is the Slack message of that reaction, with all its details"
)]
async fn i_get_the_notification_of_the_reaction(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications_until(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        1,
    )
    .await;
    assert_eq!(notifications.len(), 1);
    let notification = &notifications[0];
    assert_eq!(notification.source_item.source_id, MESSAGE_TS);
    assert_eq!(notification.title, "📥  Universal Inbox new release 📥...");
    assert_eq!(notification.kind, NotificationSourceKind::Slack);
    assert!(notification.last_read_at.is_none());
    assert!(notification.task_id.is_none());
    assert!(notification.snoozed_until.is_none());

    let ThirdPartyItemData::SlackReaction(reaction) = &notification.source_item.data else {
        unreachable!("Expected a SlackReaction data");
    };
    let SlackReactionItem::SlackMessage(message) = &reaction.item else {
        unreachable!("Expected a SlackMessage item");
    };
    assert_eq!(reaction.state, SlackReactionState::ReactionAdded);
    assert_eq!(reaction.name, "eyes".into());
    assert_eq!(
        message.url,
        "https://slack.com/archives/C05XXX/p1234567890"
            .parse()
            .unwrap()
    );
    assert_eq!(message.message.origin.ts, MESSAGE_TS.into());
    assert_eq!(message.channel.id, "C05XXX".into());
    assert_eq!(
        message.references,
        Some(SlackReferences {
            users: HashMap::from([(
                SlackUserId("U05YYY".to_string()),
                Some("john.doe".to_string()),
            )]),
            channels: HashMap::from([(
                SlackChannelId("C05XXX".to_string()),
                Some("universal-inbox".to_string()),
            )]),
            usergroups: HashMap::from([(
                SlackUserGroupId("S05ZZZ".to_string()),
                Some("admins".to_string()),
            )]),
            emojis: HashMap::from([
                (
                    SlackEmojiName("unknown1".to_string()),
                    Some(SlackEmojiRef::Alias(SlackEmojiName("wave".to_string())))
                ),
                (
                    SlackEmojiName("unknown2".to_string()),
                    Some(SlackEmojiRef::Url(
                        "https://emoji.com/unknown2.png".parse().unwrap()
                    ))
                )
            ]),
            ..SlackReferences::default()
        })
    );
    match &message.sender {
        SlackMessageSenderDetails::User(user) => {
            assert_eq!(user.expose().id, Some("U05YYY".into()));
        }
        _ => unreachable!("Expected a SlackMessageSenderDetails::User"),
    }
    assert_eq!(message.team.id, "T05XXX".into());
    assert_eq!(
        notification.get_html_url(),
        "https://slack.com/archives/C05XXX/p1234567890"
            .parse()
            .unwrap()
    );
    world.slack_tasks.notification_id = Some(notification.id);
}

#[then("I still get one unread notification, the one of that message")]
async fn i_still_get_one_notification(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications_until(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        1,
    )
    .await;
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_item.source_id, MESSAGE_TS);
}

#[then("that notification is deleted, and none is unread anymore")]
async fn that_notification_is_deleted(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    list_notifications_until(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        0,
    )
    .await;
    let notifications = list_notifications_until(
        &client,
        &app.api_address,
        vec![NotificationStatus::Deleted],
        1,
    )
    .await;
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        Some(notifications[0].id),
        world.slack_tasks.notification_id,
        "the removal deleted another notification"
    );
    assert_eq!(notifications[0].source_item.source_id, MESSAGE_TS);
    assert_eq!(
        notifications[0].title,
        "📥  Universal Inbox new release 📥..."
    );
    assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
    assert!(notifications[0].last_read_at.is_none());
    assert!(notifications[0].task_id.is_none());
    assert!(notifications[0].snoozed_until.is_none());
    let ThirdPartyItemData::SlackReaction(reaction) = &notifications[0].source_item.data else {
        unreachable!("Expected a SlackReaction data");
    };
    assert_eq!(reaction.state, SlackReactionState::ReactionRemoved);
}

#[given("I remember the unread notification of that message")]
#[then("I remember the unread notification of that message")]
async fn i_remember_the_notification(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications_until(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        1,
    )
    .await;
    assert_eq!(notifications[0].source_item.source_id, MESSAGE_TS);
    assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
    world.slack_tasks.notification_id = Some(notifications[0].id);
}

#[given("I get one active task, mirrored in Todoist, and no notification")]
#[then("I get one active task, mirrored in Todoist, and no notification")]
async fn i_get_one_active_task_and_no_notification(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let tasks = list_tasks_until(&client, &app.api_address, TaskStatus::Active, 1).await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, TaskStatus::Active);
    wait_until_the_jobs_are_done(app).await;
    let notifications =
        list_notifications(&client, &app.api_address, vec![], false, None, None, false).await;
    assert!(
        notifications.is_empty(),
        "the reaction also made a notification"
    );
    assert_eq!(tasks[0].source_item.source_id, MESSAGE_TS);
    assert_eq!(
        tasks[0].source_item.get_third_party_item_source_kind(),
        ThirdPartyItemSourceKind::SlackReaction
    );
    let sink_item = tasks[0].sink_item.as_ref().unwrap();
    assert_eq!(sink_item.source_id, todoist_item().id);
    assert_eq!(
        sink_item.get_third_party_item_source_kind(),
        ThirdPartyItemSourceKind::Todoist
    );
    let synced = list_synced_tasks(&client, &app.api_address, TaskStatus::Active, false).await;
    assert_eq!(synced.len(), 1);
    assert_eq!(synced[0].id, tasks[0].id);
    world.slack_tasks.task = Some(tasks[0].clone());
}

#[then("I still get one active task, the one of that message")]
async fn i_still_get_one_task(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let tasks = list_tasks_until(&client, &app.api_address, TaskStatus::Active, 1).await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(
        Some(tasks[0].id),
        world.slack_tasks.task.as_ref().map(|task| task.id)
    );
}

#[then("that task is done, and no notification is deleted")]
async fn that_task_is_done(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let tasks = list_tasks_until(&client, &app.api_address, TaskStatus::Done, 1).await;
    assert_eq!(tasks.len(), 1);
    wait_until_the_jobs_are_done(app).await;
    let deleted = list_notifications(
        &client,
        &app.api_address,
        vec![NotificationStatus::Deleted],
        false,
        None,
        None,
        false,
    )
    .await;
    assert!(deleted.is_empty(), "the removal deleted a notification");
    assert_eq!(
        Some(tasks[0].id),
        world.slack_tasks.task.as_ref().map(|task| task.id)
    );
    assert_eq!(tasks[0].status, TaskStatus::Done);
}

#[then("that task is active again")]
async fn that_task_is_active_again(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let tasks = list_tasks_until(&client, &app.api_address, TaskStatus::Active, 1).await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(
        Some(tasks[0].id),
        world.slack_tasks.task.as_ref().map(|task| task.id)
    );
    assert_eq!(tasks[0].status, TaskStatus::Active);
}

#[when("Slack delivers the same event again")]
async fn slack_delivers_the_same_event_again(world: &mut ApiWorld) {
    slack_delivers_the_reaction_event(world, ReactionEvent::Added).await;
}

#[given(expr = "Todoist uncompletes the task {string}")]
async fn todoist_uncompletes_the_task(world: &mut ApiWorld, id: String) {
    let app = world.app.get().await;
    expect_once(
        &app.todoist_mock_server,
        TodoistSyncPartialCommand::ItemUncomplete {
            args: TodoistSyncCommandItemUncompleteArgs { id },
        },
    )
    .await;
}

#[given(
    expr = "that other user is connected to Slack, syncing its notifications, as the Slack user {string}"
)]
async fn that_other_user_is_connected_to_slack(world: &mut ApiWorld, slack_user_id: String) {
    let user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    let mut credential = slack_oauth_credential();
    credential.provider_user_id = Some(slack_user_id);
    let app = world.app.get().await;
    create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings(),
        credential,
        None,
        Some(slack_context("T05XXX")),
    )
    .await;
}

#[then("that other user has one unread notification, which is the Slack message of that reaction")]
async fn that_other_user_has_the_notification(world: &mut ApiWorld) {
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let notifications = list_notifications_until(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        1,
    )
    .await;
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_item.source_id, MESSAGE_TS);
    assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
}

#[then("that other user has no unread notification")]
async fn that_other_user_has_no_unread_notification(world: &mut ApiWorld) {
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
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
    assert!(notifications.is_empty());
}

#[given("Slack has finished processing that event")]
#[when("Slack has finished processing that event")]
#[then("Slack has finished processing that event")]
async fn slack_has_finished_processing_that_event(world: &mut ApiWorld) {
    let app = world.app.get().await;
    wait_until_the_jobs_are_done(app).await;
}

#[given(
    "Slack serves the message of the reaction events, taking a second to answer for the message itself"
)]
async fn slack_serves_the_message_slowly(world: &mut ApiWorld) {
    mount_the_message_context(world).await;
    let app = world.app.get().await;
    Mock::given(method("GET"))
        .and(path("/conversations.replies"))
        .and(query_param("channel", "C05XXX"))
        .and(query_param("ts", MESSAGE_TS))
        .and(query_param("latest", MESSAGE_TS))
        .and(query_param("limit", "1"))
        .and(query_param("inclusive", "true"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(SLOW_ANSWER)
                .set_body_raw(
                    std::fs::read_to_string(fixture_path("slack_fetch_message_response.json"))
                        .unwrap(),
                    "application/json",
                ),
        )
        .mount(&app.slack_mock_server)
        .await;
}

#[when("I measure how long after its delivery Slack finished processing that event")]
async fn i_measure_the_wait(world: &mut ApiWorld) {
    let app = world.app.get().await;
    wait_until_the_jobs_are_done(app).await;
    let delivered_at = world
        .slack_tasks
        .delivered_at
        .expect("No event was delivered");
    world.slack_tasks.waited = Some(delivered_at.elapsed());
}

#[then("that wait lasted as long as the slow answer of Slack")]
fn the_wait_lasted_as_long_as_the_slow_answer(world: &mut ApiWorld) {
    let waited = world.slack_tasks.waited.expect("The wait was not measured");
    assert!(
        waited >= SLOW_ANSWER,
        "the wait lasted {waited:?}, less than the {SLOW_ANSWER:?} the job takes"
    );
}

#[then("my notifications list, read once without waiting, has the notification of that message")]
async fn the_list_read_once_has_the_notification(world: &mut ApiWorld) {
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
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_item.source_id, MESSAGE_TS);
}
