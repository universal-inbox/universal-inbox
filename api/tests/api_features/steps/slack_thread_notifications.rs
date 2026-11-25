use cucumber::{given, then};
use pretty_assertions::assert_eq;
use slack_morphism::prelude::SlackTs;
use wiremock::{Mock, ResponseTemplate, matchers::any};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    third_party::{integrations::slack::SlackThread, item::ThirdPartyItemData},
    user::UserId,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{create_and_mock_integration_connection, slack_oauth_credential},
        notification::slack::{create_notification_from_slack_thread, slack_thread},
    },
    world::ApiWorld,
};

async fn follow_the_thread(world: &mut ApiWorld, user_id: UserId, name: String) {
    let thread = slack_thread();
    assert_eq!((thread.subscribed, thread.last_read.clone()), (true, None));
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings(),
        slack_oauth_credential(),
        None,
        None,
    )
    .await;
    let notification =
        create_notification_from_slack_thread(app, &thread, user_id, connection.id).await;
    world.notifications.by_name.insert(name, *notification);
}

#[given(
    expr = "I have a Slack connection that syncs notifications, and the notification {string} of a Slack thread that I follow and have not read"
)]
async fn i_have_a_slack_thread_notification(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    follow_the_thread(world, user_id, name).await;
}

#[given(
    expr = "that other user has their own Slack connection, and their own notification {string} of the same Slack thread"
)]
async fn the_other_user_has_a_slack_thread_notification(world: &mut ApiWorld, name: String) {
    let user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    follow_the_thread(world, user_id, name).await;
}

#[given("Slack is not asked anything")]
async fn slack_is_not_asked_anything(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.slack_mock_server)
        .await;
}

fn the_patched_thread(world: &ApiWorld) -> SlackThread {
    let patched = world
        .notifications
        .patched
        .as_ref()
        .expect("The notification was not patched");
    let ThirdPartyItemData::SlackThread(thread) = &patched.source_item.data else {
        panic!(
            "Expected a Slack thread, got {:?}",
            patched.source_item.data
        );
    };
    (**thread).clone()
}

fn the_last_message(thread: &SlackThread) -> SlackTs {
    thread.messages.last().origin.ts.clone()
}

#[then(
    "the Slack thread of the patched notification is read up to its last message, and still followed"
)]
fn the_thread_is_read(world: &mut ApiWorld) {
    let thread = the_patched_thread(world);
    assert_eq!(
        (thread.last_read.clone(), thread.subscribed),
        (Some(the_last_message(&thread)), true)
    );
}

#[then("the Slack thread of the patched notification is no longer followed")]
fn the_thread_is_not_followed(world: &mut ApiWorld) {
    assert!(!the_patched_thread(world).subscribed);
}

#[then("the Slack thread of the patched notification is still followed and not read")]
fn the_thread_is_untouched(world: &mut ApiWorld) {
    let thread = the_patched_thread(world);
    assert_eq!((thread.last_read, thread.subscribed), (None, true));
}
