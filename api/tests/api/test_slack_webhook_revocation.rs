use chrono::{DateTime, TimeDelta, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use serde_json::json;
use slack_morphism::prelude::*;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackContext},
        provider::IntegrationConnectionContext,
    },
    user::UserId,
};
use universal_inbox_api::{
    configuration::Settings,
    repository::{
        integration_connection::{
            IntegrationConnectionRepository, SLACK_ACCESS_REVOKED_ERROR_MESSAGE,
        },
        oauth_credential::OAuthCredentialRepository,
    },
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, slack_oauth_credential,
    },
    settings,
    slack::post_signed_slack_event,
};

const TEAM_ID: &str = "T05XXX";
const OTHER_TEAM_ID: &str = "T06XXX";

fn slack_context(team_id: &str) -> IntegrationConnectionContext {
    IntegrationConnectionContext::Slack(SlackContext {
        team_id: SlackTeamId(team_id.to_string()),
        extension_credentials: vec![],
        last_extension_heartbeat_at: None,
    })
}

async fn create_slack_connection(
    app: &TestedApp,
    settings: &Settings,
    user_id: UserId,
    slack_user_id: &str,
    team_id: &str,
    mut credential: OAuthCredentialFixture,
) -> Box<IntegrationConnection> {
    credential.provider_user_id = Some(slack_user_id.to_string());
    create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::default()),
        settings,
        credential,
        None,
        Some(slack_context(team_id)),
    )
    .await
}

fn event_callback(
    team_id: &str,
    event: serde_json::Value,
    event_time: DateTime<Utc>,
) -> serde_json::Value {
    json!({
        "type": "event_callback",
        "team_id": team_id,
        "api_app_id": "A05XXX",
        "event": event,
        "event_id": "Ev05XXX",
        "event_time": event_time.timestamp(),
    })
}

fn tokens_revoked_event(
    team_id: &str,
    slack_user_ids: &[&str],
    event_time: DateTime<Utc>,
) -> serde_json::Value {
    event_callback(
        team_id,
        json!({
            "type": "tokens_revoked",
            "tokens": { "oauth": slack_user_ids, "bot": [] },
        }),
        event_time,
    )
}

fn app_uninstalled_event(team_id: &str, event_time: DateTime<Utc>) -> serde_json::Value {
    event_callback(team_id, json!({ "type": "app_uninstalled" }), event_time)
}

/// Status, failure message and whether the connection still holds a credential.
async fn connection_state(
    app: &TestedApp,
    integration_connection: &IntegrationConnection,
) -> (IntegrationConnectionStatus, Option<String>, bool) {
    let mut transaction = app.repository.begin().await.unwrap();
    let refetched = app
        .repository
        .get_integration_connection(&mut transaction, integration_connection.id)
        .await
        .unwrap()
        .expect("integration connection should still exist");
    let credential = app
        .repository
        .get_oauth_credential(&mut transaction, integration_connection.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    (
        refetched.status,
        refetched.failure_message,
        credential.is_some(),
    )
}

fn revoked() -> (IntegrationConnectionStatus, Option<String>, bool) {
    (
        IntegrationConnectionStatus::Failing,
        Some(SLACK_ACCESS_REVOKED_ERROR_MESSAGE.to_string()),
        false,
    )
}

fn untouched() -> (IntegrationConnectionStatus, Option<String>, bool) {
    (IntegrationConnectionStatus::Validated, None, true)
}

#[rstest]
#[tokio::test]
async fn test_tokens_revoked_fails_only_the_revoked_user_connection(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let (_, other_user) =
        authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
    let revoked_connection = create_slack_connection(
        &app.app,
        &settings,
        app.user.id,
        "U05XXX",
        TEAM_ID,
        slack_oauth_credential.clone(),
    )
    .await;
    let other_connection = create_slack_connection(
        &app.app,
        &settings,
        other_user.id,
        "U06XXX",
        TEAM_ID,
        slack_oauth_credential,
    )
    .await;

    let response = post_signed_slack_event(
        &app.client,
        &app.app.api_address,
        &tokens_revoked_event(TEAM_ID, &["U05XXX"], Utc::now()),
    )
    .await;

    assert_eq!(response.status(), 200);
    assert_eq!(
        connection_state(&app.app, &revoked_connection).await,
        revoked()
    );
    assert_eq!(
        connection_state(&app.app, &other_connection).await,
        untouched()
    );
}

#[rstest]
#[tokio::test]
async fn test_app_uninstalled_fails_every_connection_of_the_workspace(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let (_, other_user) =
        authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
    let (_, other_team_user) =
        authenticate_user(&app.app, "9012", "Jim", "Doe", "jim@example.com").await;
    let first_connection = create_slack_connection(
        &app.app,
        &settings,
        app.user.id,
        "U05XXX",
        TEAM_ID,
        slack_oauth_credential.clone(),
    )
    .await;
    let second_connection = create_slack_connection(
        &app.app,
        &settings,
        other_user.id,
        "U06XXX",
        TEAM_ID,
        slack_oauth_credential.clone(),
    )
    .await;
    let other_team_connection = create_slack_connection(
        &app.app,
        &settings,
        other_team_user.id,
        "U07XXX",
        OTHER_TEAM_ID,
        slack_oauth_credential,
    )
    .await;

    let response = post_signed_slack_event(
        &app.client,
        &app.app.api_address,
        &app_uninstalled_event(TEAM_ID, Utc::now()),
    )
    .await;

    assert_eq!(response.status(), 200);
    assert_eq!(
        connection_state(&app.app, &first_connection).await,
        revoked()
    );
    assert_eq!(
        connection_state(&app.app, &second_connection).await,
        revoked()
    );
    assert_eq!(
        connection_state(&app.app, &other_team_connection).await,
        untouched()
    );
}

#[rstest]
#[tokio::test]
async fn test_tokens_revoked_for_unknown_user_changes_nothing(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let connection = create_slack_connection(
        &app.app,
        &settings,
        app.user.id,
        "U05XXX",
        TEAM_ID,
        slack_oauth_credential,
    )
    .await;

    for event in [
        tokens_revoked_event(TEAM_ID, &["U99XXX"], Utc::now()),
        // Same Slack user id, but in another workspace
        tokens_revoked_event(OTHER_TEAM_ID, &["U05XXX"], Utc::now()),
        // Only bot tokens revoked: the integration has none
        tokens_revoked_event(TEAM_ID, &[], Utc::now()),
        app_uninstalled_event(OTHER_TEAM_ID, Utc::now()),
    ] {
        let response = post_signed_slack_event(&app.client, &app.app.api_address, &event).await;
        assert_eq!(response.status(), 200);
    }

    assert_eq!(connection_state(&app.app, &connection).await, untouched());
}

#[rstest]
#[tokio::test]
async fn test_replayed_and_interleaved_revocation_events_are_idempotent(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let connection = create_slack_connection(
        &app.app,
        &settings,
        app.user.id,
        "U05XXX",
        TEAM_ID,
        slack_oauth_credential,
    )
    .await;

    // Slack sends both events on uninstall, in no guaranteed order, and may
    // retry each of them.
    for event in [
        app_uninstalled_event(TEAM_ID, Utc::now()),
        tokens_revoked_event(TEAM_ID, &["U05XXX"], Utc::now()),
        tokens_revoked_event(TEAM_ID, &["U05XXX"], Utc::now()),
        app_uninstalled_event(TEAM_ID, Utc::now()),
    ] {
        let response = post_signed_slack_event(&app.client, &app.app.api_address, &event).await;
        assert_eq!(response.status(), 200);
        assert_eq!(connection_state(&app.app, &connection).await, revoked());
    }
}

#[rstest]
#[tokio::test]
async fn test_late_revocation_event_spares_a_reconnected_connection(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    // The user reconnected after the revocation: the credential is newer than
    // the event, which Slack delivers (or retries) late.
    let connection = create_slack_connection(
        &app.app,
        &settings,
        app.user.id,
        "U05XXX",
        TEAM_ID,
        slack_oauth_credential,
    )
    .await;
    let revoked_at = Utc::now() - TimeDelta::minutes(5);

    for event in [
        tokens_revoked_event(TEAM_ID, &["U05XXX"], revoked_at),
        app_uninstalled_event(TEAM_ID, revoked_at),
    ] {
        let response = post_signed_slack_event(&app.client, &app.app.api_address, &event).await;
        assert_eq!(response.status(), 200);
    }

    assert_eq!(connection_state(&app.app, &connection).await, untouched());
}
