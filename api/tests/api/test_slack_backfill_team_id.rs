#![allow(clippy::too_many_arguments)]
use pretty_assertions::assert_eq;
use rstest::*;
use serde_json::json;

use universal_inbox::integration_connection::{
    IntegrationConnectionId, IntegrationConnectionStatus, config::IntegrationConnectionConfig,
    integrations::slack::SlackConfig, provider::IntegrationConnectionContext,
};

use universal_inbox_api::{
    commands::slack::{BackfillTeamIdReport, backfill_team_id},
    configuration::Settings,
    integrations::oauth2::AccessToken,
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection,
        create_integration_connection, get_integration_connection, slack_context,
        slack_oauth_credential,
    },
    notification::slack::{mock_slack_auth_test, slack_auth_test_response},
    settings,
};

async fn run_backfill(app: &AuthenticatedApp, dry_run: bool) -> BackfillTeamIdReport {
    backfill_team_id(
        app.app.integration_connection_service.clone(),
        app.app.slack_service.clone(),
        None,
        dry_run,
    )
    .await
    .unwrap()
}

async fn get_context(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
) -> Option<IntegrationConnectionContext> {
    get_integration_connection(app, integration_connection_id)
        .await
        .unwrap()
        .provider
        .context()
}

async fn create_slack_connection(
    app: &AuthenticatedApp,
    settings: &Settings,
    credential: OAuthCredentialFixture,
) -> IntegrationConnectionId {
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        settings,
        credential,
        None,
        None,
    )
    .await
    .id
}

#[rstest]
#[tokio::test]
async fn test_backfill_team_id_for_connection_without_context(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let integration_connection_id =
        create_slack_connection(&app, &settings, slack_oauth_credential).await;
    // Called once: the second run finds no connection without context
    mock_slack_auth_test(
        &app.app.slack_mock_server,
        "slack_test_user_access_token",
        slack_auth_test_response("T053BSKET"),
        1,
    )
    .await;

    let report = run_backfill(&app, false).await;

    assert_eq!(
        report,
        BackfillTeamIdReport {
            updated: 1,
            ..Default::default()
        }
    );
    assert_eq!(
        get_context(&app, integration_connection_id).await,
        Some(slack_context("T053BSKET"))
    );

    let report = run_backfill(&app, false).await;

    assert_eq!(report, BackfillTeamIdReport::default());
    assert_eq!(
        get_context(&app, integration_connection_id).await,
        Some(slack_context("T053BSKET"))
    );
}

#[rstest]
#[tokio::test]
async fn test_backfill_team_id_dry_run(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let integration_connection_id =
        create_slack_connection(&app, &settings, slack_oauth_credential).await;
    mock_slack_auth_test(
        &app.app.slack_mock_server,
        "slack_test_user_access_token",
        slack_auth_test_response("T053BSKET"),
        1,
    )
    .await;

    let report = run_backfill(&app, true).await;

    assert_eq!(report.updated, 1);
    assert_eq!(get_context(&app, integration_connection_id).await, None);
}

#[rstest]
#[tokio::test]
async fn test_backfill_team_id_ignores_connection_with_context(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let integration_connection_id = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings,
        slack_oauth_credential,
        None,
        Some(slack_context("T01EXISTING")),
    )
    .await
    .id;
    mock_slack_auth_test(
        &app.app.slack_mock_server,
        "slack_test_user_access_token",
        slack_auth_test_response("T053BSKET"),
        0,
    )
    .await;

    let report = run_backfill(&app, false).await;

    assert_eq!(report, BackfillTeamIdReport::default());
    assert_eq!(
        get_context(&app, integration_connection_id).await,
        Some(slack_context("T01EXISTING"))
    );
}

#[rstest]
#[tokio::test]
async fn test_backfill_team_id_skips_connection_without_credential(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let integration_connection_id = create_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        IntegrationConnectionStatus::Created,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .id;
    mock_slack_auth_test(
        &app.app.slack_mock_server,
        "slack_test_user_access_token",
        slack_auth_test_response("T053BSKET"),
        0,
    )
    .await;

    let report = run_backfill(&app, false).await;

    assert_eq!(
        report,
        BackfillTeamIdReport {
            skipped_without_credential: 1,
            ..Default::default()
        }
    );
    assert_eq!(get_context(&app, integration_connection_id).await, None);
}

#[rstest]
#[tokio::test]
async fn test_backfill_team_id_skips_connection_with_expired_token(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let integration_connection_id =
        create_slack_connection(&app, &settings, slack_oauth_credential).await;
    let mut transaction = app.app.repository.begin().await.unwrap();
    sqlx::query(
        "UPDATE oauth_credential SET access_token_expires_at = NOW() - INTERVAL '1 hour' WHERE integration_connection_id = $1",
    )
    .bind(integration_connection_id.0)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    mock_slack_auth_test(
        &app.app.slack_mock_server,
        "slack_test_user_access_token",
        slack_auth_test_response("T053BSKET"),
        0,
    )
    .await;

    let report = run_backfill(&app, false).await;

    assert_eq!(
        report,
        BackfillTeamIdReport {
            failed: 1,
            ..Default::default()
        }
    );
    let integration_connection = get_integration_connection(&app, integration_connection_id)
        .await
        .unwrap();
    assert_eq!(integration_connection.provider.context(), None);
    // The command does not change the connection status
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
}

#[rstest]
#[tokio::test]
async fn test_backfill_team_id_continues_after_revoked_token(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let valid_integration_connection_id =
        create_slack_connection(&app, &settings, slack_oauth_credential.clone()).await;

    let (_, other_user) =
        authenticate_user(&app.app, "5678", "Jane", "Doe", "other@example.com").await;
    let mut revoked_credential = slack_oauth_credential;
    revoked_credential.access_token = AccessToken("slack_revoked_access_token".to_string());
    let revoked_integration_connection_id = create_and_mock_integration_connection(
        &app.app,
        other_user.id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings,
        revoked_credential,
        None,
        None,
    )
    .await
    .id;

    mock_slack_auth_test(
        &app.app.slack_mock_server,
        "slack_test_user_access_token",
        slack_auth_test_response("T053BSKET"),
        1,
    )
    .await;
    mock_slack_auth_test(
        &app.app.slack_mock_server,
        "slack_revoked_access_token",
        json!({ "ok": false, "error": "invalid_auth" }),
        1,
    )
    .await;

    let report = run_backfill(&app, false).await;

    assert_eq!(
        report,
        BackfillTeamIdReport {
            updated: 1,
            failed: 1,
            ..Default::default()
        }
    );
    assert_eq!(
        get_context(&app, valid_integration_connection_id).await,
        Some(slack_context("T053BSKET"))
    );
    assert_eq!(
        get_context(&app, revoked_integration_connection_id).await,
        None
    );
}
