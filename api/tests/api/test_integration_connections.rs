use http::StatusCode;
use rstest::*;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::google_calendar::GoogleCalendarConfig,
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

mod find_access_token {
    use chrono::{DateTime, TimeDelta, Utc};
    use pretty_assertions::assert_eq;
    use universal_inbox::user::UserId;
    use universal_inbox_api::{
        configuration::Settings,
        repository::{
            integration_connection::{
                IntegrationConnectionRepository, OAUTH_MISSING_REFRESH_TOKEN_ERROR_MESSAGE,
            },
            oauth_credential::OAuthCredentialRepository,
        },
        universal_inbox::UniversalInboxError,
        utils::crypto::{TokenEncryptionKey, encrypt_token},
    };

    use crate::helpers::{TestedApp, settings};

    use super::*;

    async fn seed_google_calendar_credential(
        app: &TestedApp,
        settings: &Settings,
        user_id: UserId,
        refresh_token: Option<&str>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Box<IntegrationConnection> {
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

        let token_encryption_key =
            TokenEncryptionKey::from_hex(&settings.oauth2.token_encryption_key).unwrap();
        let aad_context = connection.id.0.as_bytes();
        let encrypted_access_token =
            encrypt_token("expired_access_token", aad_context, &token_encryption_key).unwrap();
        let encrypted_refresh_token =
            refresh_token.map(|rt| encrypt_token(rt, aad_context, &token_encryption_key).unwrap());

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

        connection
    }

    #[rstest]
    #[tokio::test]
    async fn test_find_access_token_marks_connection_failing_when_refresh_token_missing(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;

        let connection = seed_google_calendar_credential(
            &app.app,
            &settings,
            app.user.id,
            None,
            Some(Utc::now() - TimeDelta::hours(1)),
        )
        .await;

        let service = app.app.integration_connection_service.read().await;
        let mut transaction = service.begin().await.unwrap();
        let result = service
            .find_access_token(
                &mut transaction,
                IntegrationProviderKind::GoogleCalendar,
                app.user.id,
            )
            .await;
        transaction.commit().await.unwrap();
        drop(service);

        assert!(
            matches!(result, Err(UniversalInboxError::Recoverable(_))),
            "expected Recoverable error, got {result:?}"
        );

        let mut transaction = app.app.repository.begin().await.unwrap();
        let refetched = app
            .app
            .repository
            .get_integration_connection(&mut transaction, connection.id)
            .await
            .unwrap()
            .expect("integration connection should still exist");
        transaction.commit().await.unwrap();

        assert_eq!(refetched.status, IntegrationConnectionStatus::Failing);
        assert_eq!(
            refetched.failure_message,
            Some(OAUTH_MISSING_REFRESH_TOKEN_ERROR_MESSAGE.to_string())
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_find_access_token_keeps_validated_when_refresh_token_present(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;

        let connection = seed_google_calendar_credential(
            &app.app,
            &settings,
            app.user.id,
            Some("refresh_token_present"),
            Some(Utc::now() - TimeDelta::hours(1)),
        )
        .await;

        let service = app.app.integration_connection_service.read().await;
        let mut transaction = service.begin().await.unwrap();
        let result = service
            .find_access_token(
                &mut transaction,
                IntegrationProviderKind::GoogleCalendar,
                app.user.id,
            )
            .await;
        transaction.commit().await.unwrap();
        drop(service);

        assert!(
            matches!(result, Err(UniversalInboxError::Recoverable(_))),
            "expected Recoverable error, got {result:?}"
        );

        let mut transaction = app.app.repository.begin().await.unwrap();
        let refetched = app
            .app
            .repository
            .get_integration_connection(&mut transaction, connection.id)
            .await
            .unwrap()
            .expect("integration connection should still exist");
        transaction.commit().await.unwrap();

        assert_eq!(refetched.status, IntegrationConnectionStatus::Validated);
        assert_eq!(refetched.failure_message, None);
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

    /// A connection owned by another user MUST return the same response as a
    /// completely unknown UUID — otherwise an authenticated attacker could
    /// enumerate which IntegrationConnectionId values are valid across
    /// tenants by comparing 403 (exists, foreign) vs 404 (truly missing).
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

        // Case 1: existing connection owned by another user.
        let foreign_response = client
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

        let foreign_status = foreign_response.status();
        let foreign_body = foreign_response
            .text()
            .await
            .expect("Failed to read foreign response body");

        // Case 2: completely unknown UUID — must be indistinguishable from
        // the foreign-owned case so the endpoint cannot be used to probe for
        // valid IDs across tenants.
        let unknown_id = uuid::Uuid::new_v4();
        let missing_response = client
            .put(format!(
                "{}integration-connections/{}/config",
                app.app.api_address, unknown_id
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

        let missing_status = missing_response.status();
        let missing_body = missing_response
            .text()
            .await
            .expect("Failed to read missing response body");

        // Both responses must be 404 NotFound with the same shape — only the
        // ID embedded in the message differs (which the attacker already
        // controls), so the responses carry no signal about ID validity.
        assert_eq!(foreign_status, StatusCode::NOT_FOUND);
        assert_eq!(missing_status, StatusCode::NOT_FOUND);
        assert_eq!(
            foreign_body,
            format!(
                "{{\"message\":\"Cannot update unknown integration connection {}\"}}",
                integration_connection.id
            )
        );
        assert_eq!(
            missing_body,
            format!(
                "{{\"message\":\"Cannot update unknown integration connection {unknown_id}\"}}"
            )
        );

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
