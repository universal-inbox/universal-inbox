use cucumber::{given, then};
use pretty_assertions::assert_eq;
use wiremock::{Mock, ResponseTemplate, matchers::any};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    notification::NotificationWithTask,
    third_party::{integrations::slack::SlackThread, item::ThirdPartyItemData},
    user::UserId,
};

use universal_inbox_api::integrations::oauth2::AccessToken;

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection, slack_oauth_credential,
        },
        notification::slack::{create_notification_from_slack_thread, slack_thread},
        rest::get_resource,
    },
    world::ApiWorld,
};

async fn follow_the_thread(
    world: &mut ApiWorld,
    user_id: UserId,
    credential: OAuthCredentialFixture,
    name: String,
) {
    let thread = slack_thread();
    assert_eq!((thread.subscribed, thread.last_read.clone()), (true, None));
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings(),
        credential,
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
    follow_the_thread(world, user_id, slack_oauth_credential(), name).await;
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
    let credential = OAuthCredentialFixture {
        access_token: AccessToken("other_user_slack_access_token".to_string()),
        provider_user_id: Some("U0OTHER".to_string()),
        ..slack_oauth_credential()
    };
    follow_the_thread(world, user_id, credential, name).await;
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

async fn the_stored_thread(world: &mut ApiWorld, name: &str) -> SlackThread {
    let id = world.notifications.named(name).id;
    let client = world.client();
    let app = world.app.get().await;
    let stored: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    let ThirdPartyItemData::SlackThread(thread) = stored.source_item.data.clone() else {
        panic!("Expected a Slack thread, got {:?}", stored.source_item.data);
    };
    *thread
}

#[derive(Debug, PartialEq)]
enum Read {
    Nothing,
    UpToTheLastMessage,
}

#[derive(Debug, PartialEq)]
enum Following {
    Followed,
    NotFollowed,
}

async fn the_stored_state(world: &mut ApiWorld, name: &str) -> (Read, Following) {
    let thread = the_stored_thread(world, name).await;
    let read = match &thread.last_read {
        None => Read::Nothing,
        Some(ts) if *ts == thread.messages.last().origin.ts => Read::UpToTheLastMessage,
        Some(other) => panic!("The thread is read up to an unexpected message: {other:?}"),
    };
    let following = if thread.subscribed {
        Following::Followed
    } else {
        Following::NotFollowed
    };
    (read, following)
}

#[then(
    expr = "the Slack thread of {string}, read again, is read up to its last message, and still followed"
)]
async fn the_thread_is_read(world: &mut ApiWorld, name: String) {
    assert_eq!(
        the_stored_state(world, &name).await,
        (Read::UpToTheLastMessage, Following::Followed)
    );
}

#[then(
    expr = "the Slack thread of {string}, read again, is no longer followed, and still not read"
)]
async fn the_thread_is_not_followed(world: &mut ApiWorld, name: String) {
    assert_eq!(
        the_stored_state(world, &name).await,
        (Read::Nothing, Following::NotFollowed)
    );
}

#[then(expr = "the Slack thread of {string}, read again, is still followed and not read")]
async fn the_thread_is_untouched(world: &mut ApiWorld, name: String) {
    assert_eq!(
        the_stored_state(world, &name).await,
        (Read::Nothing, Following::Followed)
    );
}
