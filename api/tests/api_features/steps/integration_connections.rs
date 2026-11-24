use chrono::{DateTime, TimeDelta, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionId,
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{github::GithubConfig, google_calendar::GoogleCalendarConfig},
        provider::IntegrationProviderKind,
    },
    notification::NotificationWithTask,
    user::UserId,
};
use universal_inbox_api::{
    repository::{
        integration_connection::{
            IntegrationConnectionRepository, OAUTH_MISSING_REFRESH_TOKEN_ERROR_MESSAGE,
        },
        oauth_credential::OAuthCredentialRepository,
    },
    universal_inbox::UniversalInboxError,
    utils::crypto::encrypt_token,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{create_integration_connection, list_integration_connections},
        notification::list_notifications,
        rest::{create_resource, delete_resource, get_resource},
    },
    steps::provider::ProviderName,
    world::ApiWorld,
};

#[given(expr = "I have created a(n) {provider} integration connection")]
async fn i_have_created_a_connection(world: &mut ApiWorld, provider: ProviderName) {
    create(world, provider).await;
}

#[when(expr = "I create a(n) {provider} integration connection")]
async fn i_create_a_connection(world: &mut ApiWorld, provider: ProviderName) {
    create(world, provider).await;
}

async fn create(world: &mut ApiWorld, provider: ProviderName) {
    let client = world.client();
    let app = world.app.get().await;
    let connection: Box<IntegrationConnection> = create_resource(
        &client,
        &app.api_address,
        "integration-connections",
        Box::new(IntegrationConnectionCreation {
            provider_kind: provider.0,
        }),
    )
    .await;
    world.connections.created.push(*connection);
}

#[then(expr = "the new connection is a(n) {provider} connection of mine that was just created")]
fn the_new_connection_is_mine(world: &mut ApiWorld, provider: ProviderName) {
    let connection = world.connections.created.last().unwrap();
    assert_eq!(connection.provider.kind(), provider.0);
    assert_eq!(connection.user_id, world.user().id);
    assert_eq!(connection.status, IntegrationConnectionStatus::Created);
}

#[when("I list my integration connections")]
async fn i_list_my_connections(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    world.connections.listed = list_integration_connections(&client, &app.api_address).await;
}

#[when("that other user lists their integration connections")]
async fn that_other_user_lists_their_connections(world: &mut ApiWorld) {
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    world.connections.listed = list_integration_connections(&client, &app.api_address).await;
}

#[then("I see no integration connections")]
fn i_see_no_connections(world: &mut ApiWorld) {
    assert!(world.connections.listed.is_empty());
}

#[then("I see exactly the connections I created, ordered by id")]
fn i_see_the_connections_i_created(world: &mut ApiWorld) {
    let mut expected = world.connections.created.clone();
    expected.sort_by_key(|connection| connection.id.0);
    assert_eq!(world.connections.listed, expected);
}

#[given("I have a validated GitHub integration connection without a stored credential")]
async fn i_have_a_validated_connection(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        IntegrationConnectionStatus::Validated,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    world.integrations.github_connection = Some(connection);
}

#[when("I disconnect my GitHub integration connection")]
async fn i_disconnect_my_github_connection(world: &mut ApiWorld) {
    let id = world.integrations.github_connection.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let disconnected: Box<IntegrationConnection> = delete_resource(
        &client,
        &app.api_address,
        "integration-connections",
        id.into(),
    )
    .await;
    world.connections.disconnected = Some(*disconnected);
}

#[then("the connection is back to created, without a failure message")]
fn the_connection_is_back_to_created(world: &mut ApiWorld) {
    let connection = world.connections.disconnected.as_ref().unwrap();
    assert_eq!(connection.status, IntegrationConnectionStatus::Created);
    assert_eq!(connection.failure_message, None);
}

#[then("my inbox holds no notification, even counting the snoozed ones and every status")]
async fn my_inbox_is_empty(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let notifications =
        list_notifications(&client, &app.api_address, vec![], true, None, None, false).await;
    assert!(notifications.is_empty());
}

#[then(expr = "I can still read the notification {string}, with its status unchanged")]
async fn i_can_still_read_the_notification(world: &mut ApiWorld, name: String) {
    let notification = world.notifications.named(&name).clone();
    let client = world.client();
    let app = world.app.get().await;
    let read: Box<NotificationWithTask> = get_resource(
        &client,
        &app.api_address,
        "notifications",
        notification.id.into(),
    )
    .await;
    assert_eq!(read.id, notification.id);
    assert_eq!(read.status, notification.status);
}

async fn seed_google_calendar_credential(
    world: &mut ApiWorld,
    refresh_token: Option<&str>,
    expires_at: Option<DateTime<Utc>>,
) {
    let user_id: UserId = world.user().id;
    let app = world.app.get().await;
    let connection = create_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        IntegrationConnectionStatus::Validated,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    let keyring = settings().data_keyring().unwrap();
    let aad = connection.id.0.as_bytes();
    let encrypted_access_token = encrypt_token("expired_access_token", aad, &keyring).unwrap();
    let encrypted_refresh_token =
        refresh_token.map(|token| encrypt_token(token, aad, &keyring).unwrap());
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .store_oauth_credential(
            &mut transaction,
            connection.id,
            encrypted_access_token,
            encrypted_refresh_token,
            expires_at,
            serde_json::json!({}),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.connections.google_calendar = Some(*connection);
}

#[given("my Google Calendar access token expired an hour ago and I have no refresh token")]
async fn the_token_expired_without_a_refresh_token(world: &mut ApiWorld) {
    seed_google_calendar_credential(world, None, Some(Utc::now() - TimeDelta::hours(1))).await;
}

#[given("my Google Calendar access token expired an hour ago and I have a refresh token")]
async fn the_token_expired_with_a_refresh_token(world: &mut ApiWorld) {
    seed_google_calendar_credential(
        world,
        Some("refresh_token_present"),
        Some(Utc::now() - TimeDelta::hours(1)),
    )
    .await;
}

#[when("the app looks for my Google Calendar access token")]
async fn the_app_looks_for_the_access_token(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let service = app.integration_connection_service.read().await;
    let mut transaction = service.begin().await.unwrap();
    let result = service
        .find_access_token(
            &mut transaction,
            IntegrationProviderKind::GoogleCalendar,
            user_id,
        )
        .await;
    transaction.commit().await.unwrap();
    drop(service);
    world.connections.token_lookup = Some(result.map(|_| String::new()));
}

#[then("the lookup fails with a recoverable error")]
fn the_lookup_fails_with_a_recoverable_error(world: &mut ApiWorld) {
    let result = world.connections.token_lookup.as_ref().unwrap();
    assert!(
        matches!(result, Err(UniversalInboxError::Recoverable(_))),
        "expected Recoverable error, got {result:?}"
    );
}

pub async fn stored_connection(
    world: &mut ApiWorld,
    id: IntegrationConnectionId,
) -> IntegrationConnection {
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connection = app
        .repository
        .get_integration_connection(&mut transaction, id)
        .await
        .unwrap()
        .expect("integration connection should still exist");
    transaction.commit().await.unwrap();
    connection
}

async fn refetched_google_calendar(world: &mut ApiWorld) -> IntegrationConnection {
    let id = world.connections.google_calendar.as_ref().unwrap().id;
    stored_connection(world, id).await
}

#[then("my Google Calendar connection is failing, because the refresh token is missing")]
async fn the_connection_is_failing(world: &mut ApiWorld) {
    let connection = refetched_google_calendar(world).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Failing);
    assert_eq!(
        connection.failure_message,
        Some(OAUTH_MISSING_REFRESH_TOKEN_ERROR_MESSAGE.to_string())
    );
}

#[then("my Google Calendar connection is still validated, without a failure message")]
async fn the_connection_is_still_validated(world: &mut ApiWorld) {
    let connection = refetched_google_calendar(world).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert_eq!(connection.failure_message, None);
}
