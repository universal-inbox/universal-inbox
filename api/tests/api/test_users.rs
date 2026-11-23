use std::collections::HashMap;

use rstest::*;
use uuid::Uuid;

use universal_inbox::{SuccessResponse, user::UserId};

use universal_inbox_api::configuration::Settings;

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    settings,
    user::{get_current_user, get_current_user_response},
};

mod delete_user {
    use super::*;
    use pretty_assertions::assert_eq;

    use chrono::Utc;
    use slack_morphism::prelude::{SlackChannelId, SlackTeamId, SlackTs};
    use universal_inbox::{
        integration_connection::{
            config::IntegrationConnectionConfig, integrations::github::GithubConfig,
        },
        slack_bridge::{SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction},
        third_party::integrations::github::GithubNotification,
    };
    use universal_inbox_api::repository::slack_bridge::SlackBridgeRepository;

    use crate::helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
        },
        notification::github::{create_notification_from_github_notification, github_notification},
        tested_app,
        user::delete_current_user_response,
    };

    /// Every `(table, column)` holding a foreign key to `"user"`, read from the
    /// live schema so a table added later is covered without touching this
    /// test.
    async fn user_foreign_keys(app: &TestedApp) -> Vec<(String, String)> {
        sqlx::query_as::<_, (String, String)>(
            r#"
                SELECT c.conrelid::regclass::text, a.attname::text
                FROM pg_constraint c
                JOIN pg_attribute a
                  ON a.attrelid = c.conrelid AND a.attnum = ANY (c.conkey)
                WHERE c.contype = 'f' AND c.confrelid = '"user"'::regclass
            "#,
        )
        .fetch_all(&*app.repository.pool)
        .await
        .unwrap()
    }

    async fn count_rows(app: &TestedApp, table: &str, column: &str, id: Uuid) -> i64 {
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE {column} = $1"
        )))
        .bind(id)
        .fetch_one(&*app.repository.pool)
        .await
        .unwrap()
    }

    async fn rows_owned_by(app: &TestedApp, user_id: UserId) -> HashMap<String, i64> {
        let mut counts = HashMap::new();
        for (table, column) in user_foreign_keys(app).await {
            let count = count_rows(app, &table, &column, user_id.0).await;
            counts.insert(format!("{table}.{column}"), count);
        }
        counts.insert(
            "user.id".to_string(),
            count_rows(app, "\"user\"", "id", user_id.0).await,
        );
        counts
    }

    /// Give the user data in as many owned tables as practical.
    async fn seed_user_data(
        app: &AuthenticatedApp,
        settings: &Settings,
        github_notification: &GithubNotification,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let response = app
            .client
            .post(format!(
                "{}users/me/authentication-tokens",
                app.app.api_address
            ))
            .header(
                reqwest::header::ORIGIN,
                crate::helpers::user::front_origin_header(&app.app),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            settings,
            github_oauth_credential,
            None,
            None,
        )
        .await;
        let notification = create_notification_from_github_notification(
            &app.app,
            github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        let now = Utc::now();
        let mut transaction = app.app.repository.begin().await.unwrap();
        app.app
            .repository
            .create_pending_action(
                &mut transaction,
                &SlackBridgePendingAction {
                    id: Uuid::new_v4().into(),
                    user_id: app.user.id,
                    notification_id: Some(notification.id),
                    action_type: SlackBridgeActionType::MarkAsRead,
                    slack_team_id: SlackTeamId::new("T1234".to_string()),
                    slack_channel_id: SlackChannelId::new("C1234".to_string()),
                    slack_thread_ts: SlackTs::new("1700000000.000100".to_string()),
                    slack_last_message_ts: SlackTs::new("1700000000.000200".to_string()),
                    status: SlackBridgeActionStatus::Pending,
                    failure_message: None,
                    retry_count: 0,
                    created_at: now,
                    updated_at: now,
                    completed_at: None,
                },
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();
    }

    #[rstest]
    #[tokio::test]
    async fn test_user_can_delete_their_account(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        seed_user_data(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;

        let owned_before = rows_owned_by(&app.app, app.user.id).await;
        for table in [
            // Recorded by the audit triggers when the notification is seeded
            "audit_event.user_id",
            "authentication_token.user_id",
            "integration_connection.user_id",
            "notification.user_id",
            "slack_bridge_pending_action.user_id",
            "third_party_item.user_id",
        ] {
            assert!(owned_before[table] > 0, "{table} should have been seeded");
        }

        let email = app.user.email.as_ref().unwrap().expose().to_string();
        let response = delete_current_user_response(&app.client, &app.app, &email).await;
        assert_eq!(response.status(), 200);
        let body: SuccessResponse = response.json().await.unwrap();
        assert!(body.success);

        // Every row owned by the user is gone.
        let owned_after = rows_owned_by(&app.app, app.user.id).await;
        assert!(
            owned_after.values().all(|count| *count == 0),
            "some user data survived the account deletion: {owned_after:?}"
        );
        let oauth_credentials: i64 = sqlx::query_scalar("SELECT count(*) FROM oauth_credential")
            .fetch_one(&*app.app.repository.pool)
            .await
            .unwrap();
        assert_eq!(oauth_credentials, 0);

        // The session was cleared.
        let response = get_current_user_response(&app.client, &app.app).await;
        assert_eq!(response.status(), 401);
    }

    #[rstest]
    #[case::empty("")]
    #[case::wrong_email("someone.else@example.com")]
    #[case::keyword("DELETE")]
    #[tokio::test]
    async fn test_account_deletion_requires_the_email_confirmation(
        #[future] authenticated_app: AuthenticatedApp,
        #[case] confirmation: &str,
    ) {
        let app = authenticated_app.await;

        let response = delete_current_user_response(&app.client, &app.app, confirmation).await;
        assert_eq!(response.status(), 400);

        let user = get_current_user(&app.client, &app.app).await;
        assert_eq!(user.id, app.user.id);
    }

    #[rstest]
    #[tokio::test]
    async fn test_account_deletion_requires_authentication(#[future] tested_app: TestedApp) {
        let app = tested_app.await;

        let response =
            delete_current_user_response(&reqwest::Client::new(), &app, "john@doe.name").await;
        assert_eq!(response.status(), 401);
    }
}
