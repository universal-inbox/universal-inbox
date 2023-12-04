use chrono::{DateTime, Utc};
use httpmock::{
    Method::{DELETE, GET},
    Mock, MockServer,
};
use reqwest::{Client, Response};
use rstest::fixture;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        NangoProviderKey,
    },
    user::UserId,
};

use universal_inbox_api::{
    configuration::Settings, integrations::oauth2::NangoConnection,
    repository::integration_connection::IntegrationConnectionRepository,
    universal_inbox::UpdateStatus,
};

use crate::helpers::{auth::AuthenticatedApp, load_json_fixture_file};

pub async fn list_integration_connections_response(client: &Client, api_address: &str) -> Response {
    client
        .get(&format!("{api_address}integration-connections"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_integration_connections(
    client: &Client,
    api_address: &str,
) -> Vec<IntegrationConnection> {
    list_integration_connections_response(client, api_address)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn verify_integration_connection_response(
    client: &Client,
    api_address: &str,
    integration_connection_id: IntegrationConnectionId,
) -> Response {
    client
        .patch(&format!(
            "{api_address}integration-connections/{integration_connection_id}/status"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn verify_integration_connection(
    client: &Client,
    api_address: &str,
    integration_connection_id: IntegrationConnectionId,
) -> IntegrationConnection {
    verify_integration_connection_response(client, api_address, integration_connection_id)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub fn mock_nango_connection_service<'a>(
    nango_mock_server: &'a MockServer,
    nango_secret_key: &str,
    connection_id: &str,
    provider_config_key: &NangoProviderKey,
    result: Box<NangoConnection>,
) -> Mock<'a> {
    nango_mock_server.mock(|when, then| {
        when.method(GET)
            .path(format!("/connection/{connection_id}"))
            .header("authorization", format!("Bearer {nango_secret_key}"))
            .query_param("provider_config_key", provider_config_key.to_string());
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(&result);
    })
}

pub fn mock_nango_delete_connection_service<'a>(
    nango_mock_server: &'a MockServer,
    nango_secret_key: &str,
    connection_id: &str,
    provider_config_key: &NangoProviderKey,
) -> Mock<'a> {
    nango_mock_server.mock(|when, then| {
        when.method(DELETE)
            .path(format!("/connection/{connection_id}"))
            .header("authorization", format!("Bearer {nango_secret_key}"))
            .query_param("provider_config_key", provider_config_key.to_string());
        then.status(204).header("content-type", "application/json");
    })
}

pub async fn create_integration_connection(
    app: &AuthenticatedApp,
    config: IntegrationConnectionConfig,
    status: IntegrationConnectionStatus,
) -> Box<IntegrationConnection> {
    let mut transaction = app.repository.begin().await.unwrap();
    let integration_connection = app
        .repository
        .create_integration_connection(
            &mut transaction,
            Box::new(IntegrationConnection::new(app.user.id, config)),
        )
        .await
        .unwrap();

    let update_result = app
        .repository
        .update_integration_connection_status(
            &mut transaction,
            integration_connection.id,
            status,
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
    with_status: Option<IntegrationConnectionStatus>,
) -> Option<IntegrationConnection> {
    let mut transaction = app.repository.begin().await.unwrap();
    let integration_connection = app
        .repository
        .get_integration_connection_per_provider(
            &mut transaction,
            user_id,
            provider_kind,
            synced_before,
            with_status,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    integration_connection
}

pub async fn get_integration_connection(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
) -> Option<IntegrationConnection> {
    let mut transaction = app.repository.begin().await.unwrap();
    let integration_connection = app
        .repository
        .get_integration_connection(&mut transaction, integration_connection_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    integration_connection
}

pub async fn update_integration_connection_context(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
    context: IntegrationConnectionContext,
) -> UpdateStatus<Box<IntegrationConnection>> {
    let mut transaction = app.repository.begin().await.unwrap();
    let result = app
        .repository
        .update_integration_connection_context(&mut transaction, integration_connection_id, context)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    result
}

pub async fn create_and_mock_integration_connection(
    app: &AuthenticatedApp,
    nango_secret_key: &str,
    config: IntegrationConnectionConfig,
    settings: &Settings,
    nango_connection: Box<NangoConnection>,
) -> Box<IntegrationConnection> {
    let provider_kind = config.kind();
    let integration_connection =
        create_integration_connection(app, config, IntegrationConnectionStatus::Validated).await;
    let config_key = settings
        .integrations
        .oauth2
        .nango_provider_keys
        .get(&provider_kind)
        .unwrap();
    mock_nango_connection_service(
        &app.nango_mock_server,
        nango_secret_key,
        &integration_connection.connection_id.to_string(),
        config_key,
        nango_connection,
    );

    integration_connection
}

#[fixture]
pub fn nango_github_connection() -> Box<NangoConnection> {
    load_json_fixture_file("/tests/api/fixtures/nango_github_connection.json")
}

#[fixture]
pub fn nango_linear_connection() -> Box<NangoConnection> {
    load_json_fixture_file("/tests/api/fixtures/nango_linear_connection.json")
}

#[fixture]
pub fn nango_google_mail_connection() -> Box<NangoConnection> {
    load_json_fixture_file("/tests/api/fixtures/nango_google_mail_connection.json")
}

#[fixture]
pub fn nango_todoist_connection() -> Box<NangoConnection> {
    load_json_fixture_file("/tests/api/fixtures/nango_todoist_connection.json")
}
