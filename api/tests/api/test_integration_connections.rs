use actix_http::StatusCode;
use httpmock::Method::GET;
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::integration_connection::{
    IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionStatus,
    IntegrationProviderKind,
};

use universal_inbox_api::{
    configuration::Settings, integrations::oauth2::NangoConnection,
    universal_inbox::integration_connection::service::UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE,
};

use crate::helpers::{
    auth::{authenticate_user, authenticated_app, AuthenticatedApp},
    integration_connection::{
        list_integration_connections, mock_nango_connection_service, nango_github_connection,
        verify_integration_connection, verify_integration_connection_response,
    },
    rest::create_resource,
    settings, tested_app, TestedApp,
};

mod list_integration_connections {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_empty_list_integration_connections(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let result = list_integration_connections(&app.client, &app.api_address).await;

        assert!(result.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_integration_connections(
        #[future] tested_app: TestedApp,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection1: Box<IntegrationConnection> = create_resource(
            &app.client,
            &app.api_address,
            "integration-connections",
            Box::new(IntegrationConnectionCreation {
                provider_kind: IntegrationProviderKind::Github,
            }),
        )
        .await;
        let integration_connection2: Box<IntegrationConnection> = create_resource(
            &app.client,
            &app.api_address,
            "integration-connections",
            Box::new(IntegrationConnectionCreation {
                provider_kind: IntegrationProviderKind::Todoist,
            }),
        )
        .await;

        let result = list_integration_connections(&app.client, &app.api_address).await;

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], *integration_connection1);
        assert_eq!(result[1], *integration_connection2);

        // Test listing notifications of another user
        let (client, _user) =
            authenticate_user(&tested_app.await, "5678", "Jane", "Doe", "jane@example.com").await;

        let result = list_integration_connections(&client, &app.api_address).await;

        assert_eq!(result.len(), 0);
    }
}

mod create_integration_connections {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_create_integration_connection(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        let integration_connection: Box<IntegrationConnection> = create_resource(
            &app.client,
            &app.api_address,
            "integration-connections",
            Box::new(IntegrationConnectionCreation {
                provider_kind: IntegrationProviderKind::Github,
            }),
        )
        .await;

        assert_eq!(
            integration_connection.provider_kind,
            IntegrationProviderKind::Github
        );
        assert_eq!(integration_connection.user_id, app.user.id);
        assert_eq!(
            integration_connection.status,
            IntegrationConnectionStatus::Created
        );
    }
}

mod verify_integration_connections {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_verify_valid_integration_connection(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection: Box<IntegrationConnection> = create_resource(
            &app.client,
            &app.api_address,
            "integration-connections",
            Box::new(IntegrationConnectionCreation {
                provider_kind: IntegrationProviderKind::Github,
            }),
        )
        .await;
        let github_config_key = settings
            .integrations
            .oauth2
            .nango_provider_keys
            .get(&IntegrationProviderKind::Github)
            .unwrap();
        let nango_mock = mock_nango_connection_service(
            &app.nango_mock_server,
            &settings.integrations.oauth2.nango_secret_key,
            &integration_connection.connection_id.to_string(),
            github_config_key,
            nango_github_connection.clone(),
        );

        let result: IntegrationConnection =
            verify_integration_connection(&app.client, &app.api_address, integration_connection.id)
                .await;

        assert_eq!(result.status, IntegrationConnectionStatus::Validated);
        assert_eq!(result.failure_message, None);
        nango_mock.assert();

        // Verifying again should keep validating the status with Nango and return the connection
        let result: IntegrationConnection =
            verify_integration_connection(&app.client, &app.api_address, integration_connection.id)
                .await;

        assert_eq!(result.status, IntegrationConnectionStatus::Validated);
        assert_eq!(result.failure_message, None);
        nango_mock.assert_hits(2);
    }

    #[rstest]
    #[tokio::test]
    async fn test_verify_unknown_integration_connection(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;

        let response = verify_integration_connection_response(
            &app.client,
            &app.api_address,
            Uuid::new_v4().into(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[rstest]
    #[tokio::test]
    async fn test_verify_unknown_integration_connection_by_nango(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_github_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection: Box<IntegrationConnection> = create_resource(
            &app.client,
            &app.api_address,
            "integration-connections",
            Box::new(IntegrationConnectionCreation {
                provider_kind: IntegrationProviderKind::Github,
            }),
        )
        .await;

        // Validate it first
        let github_config_key = settings
            .integrations
            .oauth2
            .nango_provider_keys
            .get(&IntegrationProviderKind::Github)
            .unwrap();
        let mut nango_mock = mock_nango_connection_service(
            &app.nango_mock_server,
            &settings.integrations.oauth2.nango_secret_key,
            &integration_connection.connection_id.to_string(),
            github_config_key,
            nango_github_connection.clone(),
        );

        let result: IntegrationConnection =
            verify_integration_connection(&app.client, &app.api_address, integration_connection.id)
                .await;

        assert_eq!(result.status, IntegrationConnectionStatus::Validated);
        assert_eq!(result.failure_message, None);
        nango_mock.assert();

        nango_mock.delete();
        let mut nango_mock = app.nango_mock_server.mock(|when, then| {
            when.method(GET)
                .path(format!("/connection/{}", integration_connection.connection_id))
                .header("authorization", format!("Bearer {}", settings.integrations.oauth2.nango_secret_key))
                .query_param("provider_config_key", "github");
            then.status(400).header("content-type", "application/json")
                .json_body(json!({
                    "error": "No connection matching params 'connection_id' and 'provider_config_key'.",
                    "payload": {},
                    "type": "unknown_connection"
                }));
        });

        let result: IntegrationConnection =
            verify_integration_connection(&app.client, &app.api_address, integration_connection.id)
                .await;

        assert_eq!(result.status, IntegrationConnectionStatus::Failing);
        assert_eq!(
            result.failure_message,
            Some(UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE.to_string())
        );
        nango_mock.assert();

        // Test failure recovery
        nango_mock.delete();
        let github_config_key = settings
            .integrations
            .oauth2
            .nango_provider_keys
            .get(&IntegrationProviderKind::Github)
            .unwrap();
        let nango_mock = mock_nango_connection_service(
            &app.nango_mock_server,
            &settings.integrations.oauth2.nango_secret_key,
            &integration_connection.connection_id.to_string(),
            github_config_key,
            nango_github_connection.clone(),
        );

        let result: IntegrationConnection =
            verify_integration_connection(&app.client, &app.api_address, integration_connection.id)
                .await;

        assert_eq!(result.status, IntegrationConnectionStatus::Validated);
        assert_eq!(result.failure_message, None);
        nango_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_verify_integration_connection_of_another_user(
        #[future] tested_app: TestedApp,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection: Box<IntegrationConnection> = create_resource(
            &app.client,
            &app.api_address,
            "integration-connections",
            Box::new(IntegrationConnectionCreation {
                provider_kind: IntegrationProviderKind::Github,
            }),
        )
        .await;

        let (client, _user) =
            authenticate_user(&tested_app.await, "5678", "Jane", "Doe", "jane@example.com").await;
        let response = verify_integration_connection_response(
            &client,
            &app.api_address,
            integration_connection.id,
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

mod disconnect_integration_connections {
    use crate::helpers::{
        integration_connection::{
            create_integration_connection, mock_nango_delete_connection_service,
        },
        rest::delete_resource,
    };
    use httpmock::Method::DELETE;
    use pretty_assertions::assert_eq;

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_disconnect_validated_integration_connection(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app,
            IntegrationProviderKind::Github,
            IntegrationConnectionStatus::Validated,
        )
        .await;
        let github_config_key = settings
            .integrations
            .oauth2
            .nango_provider_keys
            .get(&IntegrationProviderKind::Github)
            .unwrap();

        let nango_mock = mock_nango_delete_connection_service(
            &app.nango_mock_server,
            &settings.integrations.oauth2.nango_secret_key,
            &integration_connection.connection_id.to_string(),
            github_config_key,
        );

        let disconnected_connection: Box<IntegrationConnection> = delete_resource(
            &app.client,
            &app.api_address,
            "integration-connections",
            integration_connection.id.into(),
        )
        .await;

        assert_eq!(
            disconnected_connection.status,
            IntegrationConnectionStatus::Created
        );
        assert_eq!(disconnected_connection.failure_message, None);
        nango_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_disconnect_unknown_integration_connection_by_nango(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app,
            IntegrationProviderKind::Github,
            IntegrationConnectionStatus::Validated,
        )
        .await;

        let nango_mock = app.nango_mock_server.mock(|when, then| {
            when.method(DELETE)
                .path(format!("/connection/{}", integration_connection.connection_id))
                .header("authorization", format!("Bearer {}", settings.integrations.oauth2.nango_secret_key))
                .query_param("provider_config_key", "github");
            then.status(400).header("content-type", "application/json")
                .json_body(json!({
                    "error": "No connection matching params 'connection_id' and 'provider_config_key'.",
                    "payload": {},
                    "type": "unknown_connection"
                }));
        });

        let disconnected_connection: Box<IntegrationConnection> = delete_resource(
            &app.client,
            &app.api_address,
            "integration-connections",
            integration_connection.id.into(),
        )
        .await;

        assert_eq!(
            disconnected_connection.status,
            IntegrationConnectionStatus::Created
        );
        assert_eq!(disconnected_connection.failure_message, None);
        nango_mock.assert();
    }
}
