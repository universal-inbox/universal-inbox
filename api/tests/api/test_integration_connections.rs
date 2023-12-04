use actix_http::StatusCode;
use httpmock::Method::GET;
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
        integrations::google_mail::GoogleMailConfig, provider::IntegrationProvider,
        provider::IntegrationProviderKind, IntegrationConnection, IntegrationConnectionCreation,
        IntegrationConnectionStatus,
    },
    notification::{
        integrations::google_mail::{GoogleMailLabel, GoogleMailThread},
        Notification,
    },
};

use universal_inbox_api::{
    configuration::Settings, integrations::oauth2::NangoConnection,
    universal_inbox::integration_connection::service::UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE,
};

use crate::helpers::{
    auth::{authenticate_user, authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_integration_connection, get_integration_connection, list_integration_connections,
        mock_nango_connection_service, mock_nango_delete_connection_service,
        nango_github_connection, verify_integration_connection,
        verify_integration_connection_response,
    },
    notification::{google_mail::google_mail_thread_get_123, list_notifications},
    rest::{create_resource, delete_resource},
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
            integration_connection.provider.kind(),
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
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
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
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
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

mod update_integration_connection_config {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_update_integration_connection_config(
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
    ) {
        let app = authenticated_app.await;
        let google_mail_config = GoogleMailConfig {
            sync_notifications_enabled: true,
            synced_label: GoogleMailLabel {
                id: "Label_1".to_string(),
                name: "Label 1".to_string(),
            },
        };
        let synced_label_id = google_mail_config.synced_label.id.clone();

        let existing_notification = Box::new(google_mail_thread_get_123.into_notification(
            app.user.id,
            None,
            &synced_label_id,
        ));
        let _created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.api_address,
            "notifications",
            existing_notification,
        )
        .await;

        let integration_connection1 = create_integration_connection(
            &app,
            IntegrationConnectionConfig::GoogleMail(google_mail_config),
            IntegrationConnectionStatus::Validated,
        )
        .await;
        let integration_connection2 = create_integration_connection(
            &app,
            IntegrationConnectionConfig::Github(GithubConfig {
                sync_notifications_enabled: true,
            }),
            IntegrationConnectionStatus::Validated,
        )
        .await;

        let config: Box<IntegrationConnectionConfig> = app
            .client
            .put(&format!(
                "{}integration-connections/{}/config",
                app.api_address, integration_connection1.id
            ))
            .json(&IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                sync_notifications_enabled: false,
                synced_label: GoogleMailLabel {
                    id: "Label_2".to_string(),
                    name: "Label 2".to_string(),
                },
            }))
            .send()
            .await
            .expect("Failed to execute request")
            .json()
            .await
            .expect("Failed to parse JSON result");

        assert_eq!(
            config,
            Box::new(IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                sync_notifications_enabled: false,
                synced_label: GoogleMailLabel {
                    id: "Label_2".to_string(),
                    name: "Label 2".to_string(),
                },
            }))
        );

        // Verify the configuration has been updated
        let updated_integration_connection: Option<IntegrationConnection> =
            get_integration_connection(&app, integration_connection1.id).await;

        assert_eq!(
            updated_integration_connection,
            Some(IntegrationConnection {
                provider: IntegrationProvider::GoogleMail {
                    config: GoogleMailConfig {
                        sync_notifications_enabled: false,
                        synced_label: GoogleMailLabel {
                            id: "Label_2".to_string(),
                            name: "Label 2".to_string(),
                        }
                    },
                    context: None
                },
                ..*integration_connection1
            })
        );

        // Verify no other integration connection configuration has been updated
        let other_integration_connection: Option<IntegrationConnection> =
            get_integration_connection(&app, integration_connection2.id).await;

        assert_eq!(other_integration_connection, Some(*integration_connection2));

        // Verify notifications have been cleared
        let notifications: Vec<Notification> =
            list_notifications(&app.client, &app.api_address, vec![], true, None).await;

        assert!(notifications.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_update_integration_connection_config_of_another_user(
        #[future] tested_app: TestedApp,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app,
            IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                sync_notifications_enabled: true,
                synced_label: GoogleMailLabel {
                    id: "Label_1".to_string(),
                    name: "Label 1".to_string(),
                },
            }),
            IntegrationConnectionStatus::Validated,
        )
        .await;
        let (client, _user) =
            authenticate_user(&tested_app.await, "5678", "Jane", "Doe", "jane@example.com").await;

        let response = client
            .put(&format!(
                "{}integration-connections/{}/config",
                app.api_address, integration_connection.id
            ))
            .json(&IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                sync_notifications_enabled: false,
                synced_label: GoogleMailLabel {
                    id: "Label_2".to_string(),
                    name: "Label 2".to_string(),
                },
            }))
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        // Verify that the integration connection was not updated
        let integration_connection: IntegrationConnection =
            get_integration_connection(&app, integration_connection.id)
                .await
                .unwrap();

        assert_eq!(
            integration_connection,
            IntegrationConnection {
                provider: IntegrationProvider::GoogleMail {
                    config: GoogleMailConfig {
                        sync_notifications_enabled: true,
                        synced_label: GoogleMailLabel {
                            id: "Label_1".to_string(),
                            name: "Label 1".to_string(),
                        }
                    },
                    context: None
                },
                ..integration_connection.clone()
            }
        );
    }
}
