use chrono::{DateTime, Utc};
use httpmock::{
    Method::{DELETE, GET},
    Mock, MockServer,
};
use reqwest::{Client, Response};
use rstest::fixture;
use tracing::debug;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        IntegrationProviderKind, NangoProviderKey,
    },
    user::UserId,
};

use universal_inbox_api::{
    configuration::Settings, integrations::oauth2::NangoConnection,
    repository::integration_connection::IntegrationConnectionRepository,
};

use crate::helpers::{auth::AuthenticatedApp, load_json_fixture_file};

pub async fn list_integration_connections_response(client: &Client, app_address: &str) -> Response {
    client
        .get(&format!("{app_address}/integration-connections"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_integration_connections(
    client: &Client,
    app_address: &str,
) -> Vec<IntegrationConnection> {
    list_integration_connections_response(client, app_address)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn verify_integration_connection_response(
    client: &Client,
    app_address: &str,
    integration_connection_id: IntegrationConnectionId,
) -> Response {
    client
        .patch(&format!(
            "{app_address}/integration-connections/{integration_connection_id}/status"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn verify_integration_connection(
    client: &Client,
    app_address: &str,
    integration_connection_id: IntegrationConnectionId,
) -> IntegrationConnection {
    verify_integration_connection_response(client, app_address, integration_connection_id)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub fn mock_nango_connection_service<'a>(
    nango_mock_server: &'a MockServer,
    connection_id: &str,
    provider_config_key: &NangoProviderKey,
    result: Box<NangoConnection>,
) -> Mock<'a> {
    nango_mock_server.mock(|when, then| {
        when.method(GET)
            .path(format!("/connection/{connection_id}"))
            .header("authorization", "Basic bmFuZ29fdGVzdF9rZXk=:") // = base64("nango_test_key")
            .query_param("provider_config_key", provider_config_key.to_string());
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(&result);
    })
}

pub fn mock_nango_delete_connection_service<'a>(
    nango_mock_server: &'a MockServer,
    connection_id: &str,
    provider_config_key: &NangoProviderKey,
) -> Mock<'a> {
    nango_mock_server.mock(|when, then| {
        when.method(DELETE)
            .path(format!("/connection/{connection_id}"))
            .header("authorization", "Basic bmFuZ29fdGVzdF9rZXk=:") // = base64("nango_test_key")
            .query_param("provider_config_key", provider_config_key.to_string());
        then.status(200).header("content-type", "application/json");
    })
}

pub async fn create_validated_integration_connection(
    app: &AuthenticatedApp,
    provider_kind: IntegrationProviderKind,
) -> Box<IntegrationConnection> {
    let mut transaction = app.repository.begin().await.unwrap();
    let integration_connection = app
        .repository
        .create_integration_connection(
            &mut transaction,
            Box::new(IntegrationConnection::new(app.user.id, provider_kind)),
        )
        .await
        .unwrap();

    let update_result = app
        .repository
        .update_integration_connection_status(
            &mut transaction,
            integration_connection.id,
            IntegrationConnectionStatus::Validated,
            None,
            app.user.id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    update_result.result.unwrap()
}

pub async fn get_integration_connection_per_provider(
    app: &AuthenticatedApp,
    user_id: UserId,
    provider_kind: IntegrationProviderKind,
    synced_before: Option<DateTime<Utc>>,
) -> Option<IntegrationConnection> {
    let mut transaction = app.repository.begin().await.unwrap();
    let integration_connection = app
        .repository
        .get_integration_connection_per_provider(
            &mut transaction,
            user_id,
            provider_kind,
            synced_before,
        )
        .await
        .unwrap();
    debug!("Integration connection: {:?}", integration_connection);
    transaction.commit().await.unwrap();

    integration_connection
}

pub async fn create_and_mock_integration_connection(
    app: &AuthenticatedApp,
    provider_kind: IntegrationProviderKind,
    settings: &Settings,
    nango_connection: Box<NangoConnection>,
) -> Box<IntegrationConnection> {
    let integration_connection = create_validated_integration_connection(app, provider_kind).await;
    let github_config_key = settings
        .integrations
        .oauth2
        .nango_provider_keys
        .get(&provider_kind)
        .unwrap();
    mock_nango_connection_service(
        &app.nango_mock_server,
        &integration_connection.connection_id.to_string(),
        github_config_key,
        nango_connection,
    );

    integration_connection
}

#[fixture]
pub fn nango_github_connection() -> Box<NangoConnection> {
    load_json_fixture_file("/tests/api/fixtures/nango_github_connection.json")
}

#[fixture]
pub fn nango_todoist_connection() -> Box<NangoConnection> {
    load_json_fixture_file("/tests/api/fixtures/nango_todoist_connection.json")
}
