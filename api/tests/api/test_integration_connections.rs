use chrono::Utc;
use http::StatusCode;
use rstest::*;
use slack_morphism::prelude::SlackReactionName;
use universal_inbox_api::repository::integration_connection::IntegrationConnectionRepository;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::google_calendar::GoogleCalendarConfig,
        integrations::google_mail::GoogleMailConfig,
        integrations::slack::{SlackConfig, SlackReactionConfig},
        integrations::{github::GithubConfig, google_mail::GoogleMailContext},
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
    },
    notification::{Notification, NotificationWithTask},
    third_party::integrations::{
        github::GithubNotification,
        google_mail::{GoogleMailLabel, GoogleMailThread},
    },
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{
        create_integration_connection, get_integration_connection, list_integration_connections,
    },
    notification::{
        github::{create_notification_from_github_notification, github_notification},
        google_mail::google_mail_thread_get_123,
        list_notifications,
    },
    rest::{create_resource, delete_resource, get_resource},
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

        // The repository orders rows by id (ascending) for deterministic row-lock
        // ordering, not by insertion order, so sort the expected connections the same
        // way before comparing rather than assuming creation order.
        let mut expected = [*integration_connection1, *integration_connection2];
        expected.sort_by_key(|connection| connection.id.0);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], expected[0]);
        assert_eq!(result[1], expected[1]);

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
        github_notification: Box<GithubNotification>,
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
        let existing_notification = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            integration_connection.id,
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

        // A disconnected integration no longer feeds the inbox, so its
        // notifications are set aside rather than left stranded there — but they
        // are not removed, and reading one by id still resolves. Reconnecting
        // brings them back on the sync that reconciles them.
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

        let set_aside_notification: Box<NotificationWithTask> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            existing_notification.id.into(),
        )
        .await;

        assert_eq!(set_aside_notification.id, existing_notification.id);
        assert_eq!(set_aside_notification.status, existing_notification.status);
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
    use universal_inbox::pii::Pii;

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
                    user_email_address: Pii::<EmailAddress>::from_str("test@example.com").unwrap(),
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

        let existing_notification = create_notification_from_google_mail_thread(
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
            // Synchronizing another label narrows what belongs in the inbox.
            // Switching notifications off is deliberately not exercised here:
            // muting sets notifications aside, and that round trip has its own
            // coverage in `test_set_aside_notifications.rs`.
            .json(&IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                sync_notifications_enabled: true,
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
                sync_notifications_enabled: true,
                synced_label: GoogleMailLabel {
                    id: "Label_2".to_string(),
                    name: "Label 2".to_string(),
                },
            }))
        );

        // Verify the configuration has been updated and the existing provider
        // context is preserved (config updates must not drop provider-specific
        // state like Slack's team_id, GoogleMail's user_email_address, etc.).
        let updated_integration_connection: Option<IntegrationConnection> =
            get_integration_connection(&app, integration_connection1.id).await;

        assert_eq!(
            updated_integration_connection,
            Some(IntegrationConnection {
                provider: IntegrationProvider::GoogleMail {
                    config: GoogleMailConfig {
                        sync_notifications_enabled: true,
                        synced_label: GoogleMailLabel {
                            id: "Label_2".to_string(),
                            name: "Label 2".to_string(),
                        }
                    },
                    context: Some(GoogleMailContext {
                        user_email_address: Pii::<EmailAddress>::from_str("test@example.com")
                            .unwrap(),
                        labels: vec![],
                    }),
                },
                ..*integration_connection1
            })
        );

        // Verify no other integration connection configuration has been updated
        let other_integration_connection: Option<IntegrationConnection> =
            get_integration_connection(&app, integration_connection2.id).await;

        assert_eq!(other_integration_connection, Some(*integration_connection2));

        // Verify notifications have been preserved: saving a configuration
        // saves the configuration and nothing else. Reconciling the inbox with
        // the new configuration is the following sync's job.
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

        // `Notification`'s `PartialEq` covers the status, `last_read_at` and
        // `task_id` the user invested in it; `id` and `snoozed_until` are
        // outside it, so they are asserted on their own.
        assert_eq!(notifications, vec![*existing_notification.clone()]);
        assert_eq!(notifications[0].id, existing_notification.id);
        assert_eq!(
            notifications[0].snoozed_until,
            existing_notification.snoozed_until
        );
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

    /// A plan-paused connection must stay paused whichever toggle the incoming
    /// config flips. Slack reactions are the case to watch: they carry their
    /// own `sync_enabled`, which no notification/task accessor reports.
    #[rstest]
    #[tokio::test]
    async fn test_patch_config_cannot_re_enable_a_plan_paused_slack_reaction_sync(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Slack(SlackConfig::default()),
            IntegrationConnectionStatus::Validated,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        // Pause it the way the billing reconcile job does: marker plus the
        // pre-pause snapshot, every sync already off.
        let paused_config = IntegrationConnectionConfig::Slack(SlackConfig::default());
        let mut transaction = app.app.repository.begin().await.unwrap();
        app.app
            .repository
            .set_integration_connection_plan_pause(
                &mut transaction,
                integration_connection.id,
                Some(Utc::now()),
                Some(&paused_config),
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        let reaction_only_config = IntegrationConnectionConfig::Slack(SlackConfig {
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                ..SlackConfig::default().reaction_config
            },
            // Message sync stays off, so a guard reading only the
            // notification/task accessors sees nothing enabled.
            ..SlackConfig::default()
        });
        let response = app
            .client
            .put(format!(
                "{}integration-connections/{}/config",
                app.app.api_address, integration_connection.id
            ))
            .json(&reaction_only_config)
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);

        let stored: IntegrationConnection =
            get_integration_connection(&app, integration_connection.id)
                .await
                .unwrap();
        assert_eq!(
            stored.provider.config(),
            paused_config,
            "the refused write must not reach the database"
        );
        assert!(stored.auto_paused_by_plan_at.is_some());

        // An edit that keeps every sync off is still allowed while paused.
        let allowed_response = app
            .client
            .put(format!(
                "{}integration-connections/{}/config",
                app.app.api_address, integration_connection.id
            ))
            .json(&IntegrationConnectionConfig::Slack(SlackConfig {
                reaction_config: SlackReactionConfig {
                    reaction_name: SlackReactionName("bookmark".to_string()),
                    ..SlackConfig::default().reaction_config
                },
                ..SlackConfig::default()
            }))
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(allowed_response.status(), StatusCode::OK);
    }
}

/// Disconnecting an integration (or deleting the account) revokes the OAuth
/// grant at the provider. The revocation endpoints point at the per-test mock
/// servers (see `with_mocked_oauth_urls`).
mod revoke_provider_grants {
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use universal_inbox::integration_connection::integrations::{
        linear::LinearConfig, ticktick::TickTickConfig,
    };
    use universal_inbox_api::{
        configuration::Settings, integrations::oauth2::RefreshToken,
        repository::oauth_credential::OAuthCredentialRepository,
        universal_inbox::integration_connection::service::GrantRevocationRetryPolicy,
    };
    use uuid::Uuid;
    use wiremock::{
        Mock, MockGuard, MockServer, ResponseTemplate,
        matchers::{basic_auth, body_json, body_string_contains, header, method, path},
    };

    use super::*;
    use crate::helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            github_oauth_credential, google_mail_oauth_credential, linear_oauth_credential,
            slack_oauth_credential, ticktick_oauth_credential,
        },
        settings,
        user::delete_current_user_response,
    };

    async fn connect(
        app: &AuthenticatedApp,
        settings: &Settings,
        config: IntegrationConnectionConfig,
        credential: OAuthCredentialFixture,
    ) -> Box<IntegrationConnection> {
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            config,
            settings,
            credential,
            None,
            None,
        )
        .await
    }

    async fn disconnect(app: &AuthenticatedApp, integration_connection: &IntegrationConnection) {
        let disconnected: Box<IntegrationConnection> = delete_resource(
            &app.client,
            &app.app.api_address,
            "integration-connections",
            integration_connection.id.into(),
        )
        .await;
        assert_eq!(disconnected.status, IntegrationConnectionStatus::Created);
    }

    async fn github_revocation(server: &MockServer, settings: &Settings) -> MockGuard {
        let client_id = &settings.integrations["github"].oauth_client_id;
        Mock::given(method("DELETE"))
            .and(path(format!("/applications/{client_id}/grant")))
            .and(header("accept", "application/vnd.github+json"))
            .and(body_json(
                json!({ "access_token": "github_test_access_token" }),
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount_as_scoped(server)
            .await
    }

    async fn google_mail_revocation(server: &MockServer) -> MockGuard {
        Mock::given(method("POST"))
            .and(path("/revoke"))
            .and(body_string_contains("token=google_mail_test_refresh_token"))
            .and(body_string_contains("token_type_hint=refresh_token"))
            .respond_with(ResponseTemplate::new(200))
            .mount_as_scoped(server)
            .await
    }

    /// A row of `oauth_grant_revocation`, as the tests need it.
    #[derive(Debug, PartialEq)]
    struct Revocation {
        status: String,
        attempts: i32,
        has_tokens: bool,
        integration_connection_id: Option<Uuid>,
    }

    async fn revocations(app: &AuthenticatedApp) -> Vec<Revocation> {
        sqlx::query_as::<_, (String, i32, bool, Option<Uuid>)>(
            "SELECT status::TEXT, attempts, encrypted_access_token IS NOT NULL, integration_connection_id \
             FROM oauth_grant_revocation ORDER BY created_at",
        )
        .fetch_all(&*app.app.repository.pool)
        .await
        .unwrap()
        .into_iter()
        .map(
            |(status, attempts, has_tokens, integration_connection_id)| Revocation {
                status,
                attempts,
                has_tokens,
                integration_connection_id,
            },
        )
        .collect()
    }

    fn pending(attempts: i32, integration_connection_id: Option<Uuid>) -> Revocation {
        Revocation {
            status: "Pending".to_string(),
            attempts,
            has_tokens: true,
            integration_connection_id,
        }
    }

    fn completed(
        status: &str,
        attempts: i32,
        integration_connection_id: Option<Uuid>,
    ) -> Revocation {
        Revocation {
            status: status.to_string(),
            attempts,
            has_tokens: false,
            integration_connection_id,
        }
    }

    /// Run the `retry-oauth-grant-revocations` job body.
    async fn retry_due_revocations(app: &AuthenticatedApp, max_attempts: u32) -> (usize, usize) {
        app.app
            .integration_connection_service
            .read()
            .await
            .retry_due_grant_revocations(
                10,
                &GrantRevocationRetryPolicy {
                    base_delay_in_seconds: 60,
                    max_delay_in_seconds: 3600,
                    max_attempts,
                },
            )
            .await
            .unwrap()
    }

    /// Skip the backoff delay of every pending revocation.
    async fn make_revocations_due(app: &AuthenticatedApp) {
        sqlx::query("UPDATE oauth_grant_revocation SET next_attempt_at = NOW()")
            .execute(&*app.app.repository.pool)
            .await
            .unwrap();
    }

    async fn github_revocation_failure(server: &MockServer, settings: &Settings) -> MockGuard {
        let client_id = &settings.integrations["github"].oauth_client_id;
        Mock::given(method("DELETE"))
            .and(path(format!("/applications/{client_id}/grant")))
            .respond_with(ResponseTemplate::new(500))
            .mount_as_scoped(server)
            .await
    }

    async fn slack_revocation(
        server: &MockServer,
        access_token: &str,
        response: serde_json::Value,
    ) -> MockGuard {
        Mock::given(method("POST"))
            .and(path("/auth.revoke"))
            .and(header(
                "authorization",
                format!("Bearer {access_token}").as_str(),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount_as_scoped(server)
            .await
    }

    /// Slack token rotation: the refresh token yields a new access token.
    async fn slack_token_refresh(server: &MockServer) -> MockGuard {
        Mock::given(method("POST"))
            .and(path("/oauth.v2.access"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains(
                "refresh_token=xoxe-1-old-refresh-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "access_token": "xoxe.xoxp-new-access-token",
                "refresh_token": "xoxe-1-new-refresh-token",
                "expires_in": 43200,
                "token_type": "user"
            })))
            .mount_as_scoped(server)
            .await
    }

    fn rotating_slack_oauth_credential() -> OAuthCredentialFixture {
        OAuthCredentialFixture {
            refresh_token: Some(RefreshToken("xoxe-1-old-refresh-token".to_string())),
            ..slack_oauth_credential()
        }
    }

    async fn expire_access_token(
        app: &AuthenticatedApp,
        integration_connection: &IntegrationConnection,
    ) {
        sqlx::query(
            "UPDATE oauth_credential SET access_token_expires_at = NOW() - INTERVAL '1 hour' \
             WHERE integration_connection_id = $1",
        )
        .bind(integration_connection.id.0)
        .execute(&*app.app.repository.pool)
        .await
        .unwrap();
    }

    async fn assert_called_once(guard: &MockGuard, provider: &str) {
        assert_eq!(
            guard.received_requests().await.len(),
            1,
            "the {provider} grant should have been revoked"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_disconnecting_github_revokes_the_grant(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            github_oauth_credential(),
        )
        .await;
        let guard = github_revocation(&app.app.github_mock_server, &settings).await;

        disconnect(&app, &connection).await;

        assert_called_once(&guard, "GitHub").await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_disconnecting_google_revokes_the_grant(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
            google_mail_oauth_credential(),
        )
        .await;
        let guard = google_mail_revocation(&app.app.google_mail_mock_server).await;

        disconnect(&app, &connection).await;

        assert_called_once(&guard, "Google").await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_disconnecting_slack_revokes_the_token(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Slack(SlackConfig::default()),
            slack_oauth_credential(),
        )
        .await;
        let guard = Mock::given(method("POST"))
            .and(path("/auth.revoke"))
            .and(header(
                "authorization",
                "Bearer slack_test_user_access_token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "revoked": true
            })))
            .mount_as_scoped(&app.app.slack_mock_server)
            .await;

        disconnect(&app, &connection).await;

        assert_called_once(&guard, "Slack").await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_disconnecting_linear_revokes_the_grant(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            linear_oauth_credential(),
        )
        .await;
        let guard = Mock::given(method("POST"))
            .and(path("/oauth/revoke"))
            .and(body_string_contains("token=linear_test_refresh_token"))
            .respond_with(ResponseTemplate::new(200))
            .mount_as_scoped(&app.app.linear_mock_server)
            .await;

        disconnect(&app, &connection).await;

        assert_called_once(&guard, "Linear").await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_disconnecting_ticktick_revokes_the_token(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            ticktick_oauth_credential(),
        )
        .await;
        let ticktick_settings = &settings.integrations["ticktick"];
        let guard = Mock::given(method("POST"))
            .and(path("/oauth/revoke"))
            .and(basic_auth(
                &ticktick_settings.oauth_client_id,
                ticktick_settings.oauth_client_secret.as_str(),
            ))
            .and(body_string_contains("token=ticktick_test_access_token"))
            .respond_with(ResponseTemplate::new(200))
            .mount_as_scoped(&app.app.ticktick_mock_server)
            .await;

        disconnect(&app, &connection).await;

        assert_called_once(&guard, "TickTick").await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_a_failed_revocation_is_queued_then_retried(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            github_oauth_credential(),
        )
        .await;
        let failure_guard = github_revocation_failure(&app.app.github_mock_server, &settings).await;

        // The provider failure does not block the disconnect...
        disconnect(&app, &connection).await;

        assert_called_once(&failure_guard, "GitHub").await;
        // ...the token is queued instead of being lost with the credential.
        assert_eq!(
            revocations(&app).await,
            vec![pending(1, Some(connection.id.0))]
        );
        drop(failure_guard);

        let guard = github_revocation(&app.app.github_mock_server, &settings).await;
        make_revocations_due(&app).await;
        assert_eq!(retry_due_revocations(&app, 20).await, (1, 0));

        assert_called_once(&guard, "GitHub").await;
        assert_eq!(
            revocations(&app).await,
            vec![completed("Revoked", 1, Some(connection.id.0))]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_a_failing_revocation_backs_off_then_is_abandoned(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            github_oauth_credential(),
        )
        .await;
        let _failure_guard =
            github_revocation_failure(&app.app.github_mock_server, &settings).await;
        disconnect(&app, &connection).await;

        make_revocations_due(&app).await;
        assert_eq!(retry_due_revocations(&app, 3).await, (0, 1));
        assert_eq!(
            revocations(&app).await,
            vec![pending(2, Some(connection.id.0))]
        );

        // Backing off: not due again yet.
        assert_eq!(retry_due_revocations(&app, 3).await, (0, 0));

        make_revocations_due(&app).await;
        assert_eq!(retry_due_revocations(&app, 3).await, (0, 1));
        assert_eq!(
            revocations(&app).await,
            vec![Revocation {
                status: "Abandoned".to_string(),
                attempts: 3,
                // Kept, so an operator can still retry by hand.
                has_tokens: true,
                integration_connection_id: Some(connection.id.0),
            }]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_an_expired_slack_token_is_refreshed_then_revoked(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Slack(SlackConfig::default()),
            rotating_slack_oauth_credential(),
        )
        .await;
        expire_access_token(&app, &connection).await;
        let refresh_guard = slack_token_refresh(&app.app.slack_mock_server).await;
        let revoke_guard = slack_revocation(
            &app.app.slack_mock_server,
            "xoxe.xoxp-new-access-token",
            json!({ "ok": true, "revoked": true }),
        )
        .await;

        disconnect(&app, &connection).await;

        assert_called_once(&refresh_guard, "Slack refresh").await;
        assert_called_once(&revoke_guard, "Slack").await;
        assert_eq!(revocations(&app).await, vec![]);
    }

    #[rstest]
    #[tokio::test]
    async fn test_a_queued_slack_token_expired_at_retry_is_refreshed_then_revoked(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Slack(SlackConfig::default()),
            rotating_slack_oauth_credential(),
        )
        .await;
        let failure_guard = Mock::given(method("POST"))
            .and(path("/auth.revoke"))
            .respond_with(ResponseTemplate::new(503))
            .mount_as_scoped(&app.app.slack_mock_server)
            .await;
        disconnect(&app, &connection).await;
        assert_eq!(
            revocations(&app).await,
            vec![pending(1, Some(connection.id.0))]
        );
        drop(failure_guard);

        // Meanwhile the access token expired, which only Slack knows.
        let expired_guard = slack_revocation(
            &app.app.slack_mock_server,
            "slack_test_user_access_token",
            json!({ "ok": false, "error": "token_expired" }),
        )
        .await;
        let refresh_guard = slack_token_refresh(&app.app.slack_mock_server).await;
        let revoke_guard = slack_revocation(
            &app.app.slack_mock_server,
            "xoxe.xoxp-new-access-token",
            json!({ "ok": true, "revoked": true }),
        )
        .await;
        make_revocations_due(&app).await;
        assert_eq!(retry_due_revocations(&app, 20).await, (1, 0));

        assert_called_once(&expired_guard, "Slack (expired)").await;
        assert_called_once(&refresh_guard, "Slack refresh").await;
        assert_called_once(&revoke_guard, "Slack").await;
        assert_eq!(
            revocations(&app).await,
            vec![completed("Revoked", 1, Some(connection.id.0))]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_an_already_dead_slack_token_counts_as_revoked(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Slack(SlackConfig::default()),
            slack_oauth_credential(),
        )
        .await;
        let guard = slack_revocation(
            &app.app.slack_mock_server,
            "slack_test_user_access_token",
            json!({ "ok": false, "error": "token_revoked" }),
        )
        .await;

        disconnect(&app, &connection).await;

        assert_called_once(&guard, "Slack").await;
        assert_eq!(revocations(&app).await, vec![]);
    }

    #[rstest]
    #[tokio::test]
    async fn test_a_reconnect_cancels_the_pending_revocation(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            github_oauth_credential(),
        )
        .await;
        let failure_guard = github_revocation_failure(&app.app.github_mock_server, &settings).await;
        disconnect(&app, &connection).await;
        drop(failure_guard);

        // The user connects GitHub again: GitHub revokes the whole grant, so
        // the queued revocation would kill the new token too.
        let mut transaction = app.app.repository.begin().await.unwrap();
        app.app
            .repository
            .store_oauth_credential(
                &mut transaction,
                connection.id,
                b"new encrypted token".to_vec(),
                None,
                None,
                json!({}),
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        let guard = github_revocation(&app.app.github_mock_server, &settings).await;

        make_revocations_due(&app).await;
        assert_eq!(retry_due_revocations(&app, 20).await, (1, 0));

        assert!(
            guard.received_requests().await.is_empty(),
            "the reconnected GitHub grant must not be revoked"
        );
        assert_eq!(
            revocations(&app).await,
            vec![completed("Cancelled", 1, Some(connection.id.0))]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_account_deletion_queues_the_failed_revocations(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            github_oauth_credential(),
        )
        .await;
        let failure_guard = github_revocation_failure(&app.app.github_mock_server, &settings).await;

        let email = app.user.email.as_ref().unwrap().expose().to_string();
        let response = delete_current_user_response(&app.client, &app.app, &email).await;
        assert_eq!(response.status(), StatusCode::OK);

        // The connection is gone with the user, the queued token is not.
        assert_eq!(revocations(&app).await, vec![pending(1, None)]);
        drop(failure_guard);

        let guard = github_revocation(&app.app.github_mock_server, &settings).await;
        make_revocations_due(&app).await;
        assert_eq!(retry_due_revocations(&app, 20).await, (1, 0));

        assert_called_once(&guard, "GitHub").await;
        assert_eq!(revocations(&app).await, vec![completed("Revoked", 1, None)]);
    }

    #[rstest]
    #[tokio::test]
    async fn test_account_deletion_revokes_every_grant(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect(
            &app,
            &settings,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            github_oauth_credential(),
        )
        .await;
        connect(
            &app,
            &settings,
            IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
            google_mail_oauth_credential(),
        )
        .await;
        let github_guard = github_revocation(&app.app.github_mock_server, &settings).await;
        let google_guard = google_mail_revocation(&app.app.google_mail_mock_server).await;

        let email = app.user.email.as_ref().unwrap().expose().to_string();
        let response = delete_current_user_response(&app.client, &app.app, &email).await;
        assert_eq!(response.status(), StatusCode::OK);

        assert_called_once(&github_guard, "GitHub").await;
        assert_called_once(&google_guard, "Google").await;
    }
}

mod oauth_credential_storage {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use universal_inbox_api::repository::oauth_credential::OAuthCredentialRepository;

    /// A provider token response must never be persisted with its cleartext
    /// credentials: they live encrypted in their own columns.
    #[rstest]
    #[tokio::test]
    async fn test_store_oauth_credential_strips_cleartext_tokens(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            IntegrationConnectionStatus::Validated,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        let mut transaction = app.app.repository.begin().await.unwrap();
        let stored = app
            .app
            .repository
            .store_oauth_credential(
                &mut transaction,
                integration_connection.id,
                vec![1, 2, 3],
                None,
                None,
                json!({
                    "ok": true,
                    "access_token": "xoxe.xoxp-cleartext",
                    "refresh_token": "xoxe-1-cleartext",
                    "authed_user": { "id": "U1", "access_token": "xoxp-cleartext" }
                }),
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        assert_eq!(
            stored.raw_token_response,
            json!({ "ok": true, "authed_user": { "id": "U1" } })
        );
    }
}
