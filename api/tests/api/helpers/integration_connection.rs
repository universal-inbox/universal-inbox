use httpmock::{
    Method::{DELETE, GET},
    Mock, MockServer,
};
use reqwest::{Client, Response};

use rstest::fixture;
use universal_inbox::integration_connection::{
    IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionId,
    IntegrationConnectionStatus, IntegrationProviderKind, NangoProviderKey,
};

use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};

use crate::helpers::{load_json_fixture_file, rest::create_resource};

use super::auth::AuthenticatedApp;

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
    settings: &Settings,
    app: &AuthenticatedApp,
    nango_connection: Box<NangoConnection>,
    provider_kind: IntegrationProviderKind,
) -> IntegrationConnection {
    let integration_connection: Box<IntegrationConnection> = create_resource(
        &app.client,
        &app.app_address,
        "integration-connections",
        Box::new(IntegrationConnectionCreation { provider_kind }),
    )
    .await;
    let github_config_key = settings
        .integrations
        .oauth2
        .nango_provider_keys
        .get(&provider_kind)
        .unwrap();
    let mut nango_mock = mock_nango_connection_service(
        &app.nango_mock_server,
        &integration_connection.connection_id.to_string(),
        github_config_key,
        nango_connection.clone(),
    );

    let result: IntegrationConnection =
        verify_integration_connection(&app.client, &app.app_address, integration_connection.id)
            .await;

    assert_eq!(result.status, IntegrationConnectionStatus::Validated);
    assert_eq!(result.failure_message, None);
    nango_mock.assert();
    nango_mock.delete();

    result
}

#[fixture]
pub fn nango_connection() -> Box<NangoConnection> {
    load_json_fixture_file("/tests/api/fixtures/nango_connection.json")
}
