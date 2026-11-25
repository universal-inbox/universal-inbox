use cucumber::{given, when};
use wiremock::{Mock, ResponseTemplate, matchers::any};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    notification::{NotificationStatus, service::NotificationPatch},
    third_party::item::ThirdPartyItemData,
};
use universal_inbox_api::integrations::oauth2::AccessToken;

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            todoist_oauth_credential,
        },
        notification::todoist::create_notification_from_todoist_item,
        rest::patch_resource_response,
        task::todoist::todoist_item,
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

#[given(
    expr = "that other user has their own Todoist connection, and their own notification {string} of the same Todoist item"
)]
async fn the_other_user_has_the_same_todoist_item(world: &mut ApiWorld, name: String) {
    let user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    let (_, mine) = world
        .todoist_task_changes
        .items
        .last()
        .expect("I have no Todoist task");
    let ThirdPartyItemData::TodoistItem(item) = mine.third_party_item.data.clone() else {
        panic!("My task is not a Todoist item");
    };
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings(),
        OAuthCredentialFixture {
            access_token: AccessToken("other_user_todoist_access_token".to_string()),
            ..todoist_oauth_credential()
        },
        None,
        None,
    )
    .await;
    let notification =
        create_notification_from_todoist_item(app, &item, user_id, connection.id).await;
    world.notifications.by_name.insert(name, *notification);
}

#[given("Todoist is not asked anything more")]
async fn todoist_is_not_asked_anything_more(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.todoist_mock_server)
        .await;
}
