use cucumber::given;
use serde_json::json;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{any, body_string, header, method, path},
};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{google_drive::GoogleDriveConfig, google_mail::GoogleMailConfig},
    },
    third_party::integrations::google_mail::{GOOGLE_MAIL_INBOX_LABEL, GoogleMailThread},
    user::UserId,
};
use universal_inbox_api::integrations::oauth2::AccessToken;

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            google_drive_oauth_credential, google_mail_oauth_credential,
        },
        notification::{
            google_drive::{
                create_notification_from_google_drive_comment, google_drive_comment_123,
            },
            google_mail::{
                create_notification_from_google_mail_thread, google_mail_thread_get_123,
                google_mail_user_profile, raw_google_mail_thread_get_123,
            },
        },
    },
    world::ApiWorld,
};

const MY_GOOGLE_MAIL_TOKEN: &str = "google_mail_test_access_token";

fn thread_123() -> GoogleMailThread {
    google_mail_thread_get_123(raw_google_mail_thread_get_123(), google_mail_user_profile())
}

fn the_other_user(world: &ApiWorld) -> UserId {
    world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id
}

#[given(expr = "I have the notification {string} of the Google Mail thread 123")]
async fn i_have_the_notification_of_thread_123(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .google_mail_connection
        .as_ref()
        .expect("The account is not connected to Google Mail")
        .id;
    let app = world.app.get().await;
    let notification =
        create_notification_from_google_mail_thread(app, &thread_123(), user_id, connection_id)
            .await;
    world.notifications.by_name.insert(name, *notification);
}

#[given(
    expr = "that other user has their own Google Mail connection, and their own notification {string} of the same thread"
)]
async fn the_other_user_has_the_mail_thread(world: &mut ApiWorld, name: String) {
    let user_id = the_other_user(world);
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        &settings(),
        OAuthCredentialFixture {
            access_token: AccessToken("other_user_google_mail_access_token".to_string()),
            ..google_mail_oauth_credential()
        },
        None,
        None,
    )
    .await;
    let notification =
        create_notification_from_google_mail_thread(app, &thread_123(), user_id, connection.id)
            .await;
    world.notifications.by_name.insert(name, *notification);
}

#[given(
    expr = "that other user has their own Google Drive connection, and their own notification {string} of the same comment"
)]
async fn the_other_user_has_the_drive_comment(world: &mut ApiWorld, name: String) {
    let user_id = the_other_user(world);
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled()),
        &settings(),
        OAuthCredentialFixture {
            access_token: AccessToken("other_user_google_drive_access_token".to_string()),
            ..google_drive_oauth_credential()
        },
        None,
        None,
    )
    .await;
    let notification = create_notification_from_google_drive_comment(
        app,
        &google_drive_comment_123(),
        user_id,
        connection.id,
    )
    .await;
    world.notifications.by_name.insert(name, *notification);
}

#[given(
    "Google Mail archives, once and with my access token, the thread 123: it removes the inbox label and the synced label, and adds none"
)]
async fn google_mail_archives_the_thread(world: &mut ApiWorld) {
    let synced_label_id = GoogleMailConfig::enabled().synced_label.id;
    let app = world.app.get().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/users/me/threads/{}/modify",
            thread_123().id
        )))
        .and(body_string(
            json!({
                "addLabelIds": [],
                "removeLabelIds": [GOOGLE_MAIL_INBOX_LABEL, synced_label_id]
            })
            .to_string(),
        ))
        .and(header(
            "authorization",
            format!("Bearer {MY_GOOGLE_MAIL_TOKEN}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "application/json"))
        .expect(1)
        .mount(&app.google_mail_mock_server)
        .await;
}

#[given("Google Mail refuses, once, to modify the thread 123")]
async fn google_mail_refuses_to_modify_the_thread(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/users/me/threads/{}/modify",
            thread_123().id
        )))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&app.google_mail_mock_server)
        .await;
}

#[given("Google Drive is not asked anything")]
async fn google_drive_is_not_asked_anything(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.google_drive_mock_server)
        .await;
}
