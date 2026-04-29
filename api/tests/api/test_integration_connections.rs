use http::StatusCode;
use rstest::*;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::google_mail::GoogleMailConfig,
        integrations::{github::GithubConfig, google_mail::GoogleMailContext},
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
    },
    notification::Notification,
    third_party::integrations::google_mail::{GoogleMailLabel, GoogleMailThread},
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{
        create_integration_connection, get_integration_connection, list_integration_connections,
    },
    notification::{google_mail::google_mail_thread_get_123, list_notifications},
    rest::{create_resource, delete_resource},
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
        let result = list_integration_connections(&app.client, &app.app.api_address).await;

        assert!(result.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_integration_connections(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let integration_connection1: Box<IntegrationConnection> = create_resource(
            &app.client,
            &app.app.api_address,
            "integration-connections",
            Box::new(IntegrationConnectionCreation {
                provider_kind: IntegrationProviderKind::Github,
            }),
        )
        .await;
        let integration_connection2: Box<IntegrationConnection> = create_resource(
            &app.client,
            &app.app.api_address,
            "integration-connections",
            Box::new(IntegrationConnectionCreation {
                provider_kind: IntegrationProviderKind::Todoist,
            }),
        )
        .await;

        let result = list_integration_connections(&app.client, &app.app.api_address).await;

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], *integration_connection1);
        assert_eq!(result[1], *integration_connection2);

        // Test listing notifications of another user
        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;

        let result = list_integration_connections(&client, &app.app.api_address).await;

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
            &app.app.api_address,
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

mod disconnect_integration_connections {
    use pretty_assertions::assert_eq;

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_disconnect_validated_integration_connection(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            IntegrationConnectionStatus::Validated,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        let disconnected_connection: Box<IntegrationConnection> = delete_resource(
            &app.client,
            &app.app.api_address,
            "integration-connections",
            integration_connection.id.into(),
        )
        .await;

        assert_eq!(
            disconnected_connection.status,
            IntegrationConnectionStatus::Created
        );
        assert_eq!(disconnected_connection.failure_message, None);
    }
}

mod update_integration_connection_config {
    use std::str::FromStr;

    use email_address::EmailAddress;

    use crate::helpers::notification::google_mail::create_notification_from_google_mail_thread;

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

        let integration_connection1 = create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::GoogleMail(google_mail_config),
            IntegrationConnectionStatus::Validated,
            Some(IntegrationConnectionContext::GoogleMail(
                GoogleMailContext {
                    user_email_address: EmailAddress::from_str("test@example.com").unwrap(),
                    labels: vec![],
                },
            )),
            None,
            None,
            None,
            None,
        )
        .await;
        let integration_connection2 = create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig {
                sync_notifications_enabled: true,
            }),
            IntegrationConnectionStatus::Validated,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        create_notification_from_google_mail_thread(
            &app.app,
            &google_mail_thread_get_123,
            app.user.id,
            integration_connection1.id,
        )
        .await;

        let config: Box<IntegrationConnectionConfig> = app
            .client
            .put(format!(
                "{}integration-connections/{}/config",
                app.app.api_address, integration_connection1.id
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

        // Verify the configuration has been updated and context has been cleared
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
        let notifications: Vec<Notification> = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![],
            true,
            None,
            None,
            false,
        )
        .await;

        assert!(notifications.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_update_integration_connection_config_of_another_user(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                sync_notifications_enabled: true,
                synced_label: GoogleMailLabel {
                    id: "Label_1".to_string(),
                    name: "Label 1".to_string(),
                },
            }),
            IntegrationConnectionStatus::Validated,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        let (client, _user) =
            authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;

        let response = client
            .put(format!(
                "{}integration-connections/{}/config",
                app.app.api_address, integration_connection.id
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
