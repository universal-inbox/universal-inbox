//! Providers keep a grant valid as long as its token is (and Slack keeps
//! sending events for it), so the OAuth connections of users inactive for too
//! long, and those failing for too long, are paused: their grant is revoked at
//! the provider, their credential deleted, and they stay `Paused` until the
//! user reconnects them. Inactive users are warned by email before the pause,
//! and every user is emailed once paused.

use chrono::{DateTime, TimeDelta, Utc};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;
use rstest::*;
use serde_json::json;
use universal_inbox::pii::Pii;
use wiremock::{
    Mock, MockGuard, ResponseTemplate,
    matchers::{body_string_contains, header, method, path},
};

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionPausedReason, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{linear::LinearConfig, slack::SlackConfig},
    },
    user::UserId,
};

use universal_inbox_api::{
    commands::integration_connection::pause_without_email,
    configuration::Settings,
    integrations::oauth2::RefreshToken,
    jobs::oauth::pause_integration_connections,
    mailer::EmailTemplate,
    repository::{
        integration_connection::IntegrationConnectionRepository,
        oauth_credential::{OAuthCredentialRepository, StoredOAuthCredential},
    },
    universal_inbox::integration_connection::service::PauseWithoutEmailReport,
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection,
        linear_oauth_credential, slack_oauth_credential,
    },
    rest::delete_resource,
    settings, tested_app_with_local_auth,
    user::{create_user, get_current_user, login_user_response},
};

const INACTIVITY_THRESHOLD_DAYS: i64 = 90;
const INACTIVITY_WARNING_DAYS: i64 = 7;
const NO_INACTIVITY_WARNING: i64 = 0;
const FAILING_THRESHOLD_DAYS: i64 = 30;

async fn connect_slack(
    app: &AuthenticatedApp,
    settings: &Settings,
    refresh_token: Option<&str>,
) -> Box<IntegrationConnection> {
    let mut credential = slack_oauth_credential();
    credential.refresh_token = refresh_token.map(|token| RefreshToken(token.to_string()));
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
}

async fn connect_linear(app: &AuthenticatedApp, settings: &Settings) -> Box<IntegrationConnection> {
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
        settings,
        linear_oauth_credential(),
        None,
        None,
    )
    .await
}

async fn mock_linear_revocation(app: &AuthenticatedApp) -> MockGuard {
    Mock::given(method("POST"))
        .and(path("/oauth/revoke"))
        .and(body_string_contains("token=linear_test_refresh_token"))
        .respond_with(ResponseTemplate::new(200))
        .mount_as_scoped(&app.app.linear_mock_server)
        .await
}

async fn set_last_active_at(app: &TestedApp, user_id: UserId, last_active_at: DateTime<Utc>) {
    sqlx::query(r#"UPDATE "user" SET last_active_at = $2 WHERE id = $1"#)
        .bind(user_id.0)
        .bind(last_active_at.naive_utc())
        .execute(&*app.repository.pool)
        .await
        .expect("Failed to set the user last activity");
}

async fn fetch_last_active_at(app: &TestedApp, user_id: UserId) -> DateTime<Utc> {
    sqlx::query_scalar::<_, chrono::NaiveDateTime>(
        r#"SELECT last_active_at FROM "user" WHERE id = $1"#,
    )
    .bind(user_id.0)
    .fetch_one(&*app.repository.pool)
    .await
    .map(|last_active_at| last_active_at.and_utc())
    .expect("Failed to fetch the user last activity")
}

async fn fetch_oauth_credential(
    app: &AuthenticatedApp,
    integration_connection: &IntegrationConnection,
) -> Option<StoredOAuthCredential> {
    let mut transaction = app.app.repository.begin().await.unwrap();
    let credential = app
        .app
        .repository
        .get_oauth_credential(&mut transaction, integration_connection.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    credential
}

async fn expire_access_token(
    app: &AuthenticatedApp,
    integration_connection: &IntegrationConnection,
) {
    sqlx::query(
        "UPDATE oauth_credential SET access_token_expires_at = $2 WHERE integration_connection_id = $1",
    )
    .bind(integration_connection.id.0)
    .bind(Utc::now() - TimeDelta::hours(1))
    .execute(&*app.app.repository.pool)
    .await
    .expect("Failed to expire the access token");
}

async fn mock_slack_revocation(app: &AuthenticatedApp, access_token: &str) -> MockGuard {
    Mock::given(method("POST"))
        .and(path("/auth.revoke"))
        .and(header("authorization", format!("Bearer {access_token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "revoked": true
        })))
        .mount_as_scoped(&app.app.slack_mock_server)
        .await
}

async fn pause_inactive_users_connections(app: &AuthenticatedApp) -> (usize, usize) {
    app.app
        .integration_connection_service
        .read()
        .await
        .pause_integration_connections_of_inactive_users(
            Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS),
            None,
        )
        .await
        .unwrap()
}

async fn run_pause_integration_connections(app: &AuthenticatedApp, inactivity_warning_days: i64) {
    pause_integration_connections(
        app.app.integration_connection_service.clone(),
        INACTIVITY_THRESHOLD_DAYS,
        inactivity_warning_days,
        FAILING_THRESHOLD_DAYS,
    )
    .await
    .unwrap();
}

/// The emails sent about pausing connections, oldest first.
async fn pause_emails(app: &AuthenticatedApp) -> Vec<EmailTemplate> {
    app.app
        .mailer_stub
        .read()
        .await
        .emails_sent
        .read()
        .await
        .iter()
        .filter(|(_, template)| {
            matches!(
                template,
                EmailTemplate::IntegrationConnectionPauseWarning { .. }
                    | EmailTemplate::IntegrationConnectionPaused { .. }
            )
        })
        .map(|(_, template)| template.clone())
        .collect()
}

async fn set_inactivity_warning_sent_at(
    app: &AuthenticatedApp,
    integration_connection: &IntegrationConnection,
    sent_at: DateTime<Utc>,
) {
    sqlx::query("UPDATE integration_connection SET inactivity_warning_sent_at = $2 WHERE id = $1")
        .bind(integration_connection.id.0)
        .bind(sent_at.naive_utc())
        .execute(&*app.app.repository.pool)
        .await
        .expect("Failed to set the inactivity warning time");
}

fn assert_paused_email(
    app: &AuthenticatedApp,
    template: &EmailTemplate,
    expected_reason: IntegrationConnectionPausedReason,
) {
    assert_provider_paused_email(app, template, &["Slack"], expected_reason);
}

fn assert_provider_paused_email(
    app: &AuthenticatedApp,
    template: &EmailTemplate,
    expected_provider_names: &[&str],
    expected_reason: IntegrationConnectionPausedReason,
) {
    let EmailTemplate::IntegrationConnectionPaused {
        provider_names,
        paused_reason,
        reconnect_url,
        ..
    } = template
    else {
        panic!("Expected a paused email, got {template:?}");
    };
    assert_eq!(provider_names, expected_provider_names);
    assert_eq!(*paused_reason, expected_reason);
    assert_eq!(
        *reconnect_url,
        app.app.front_base_url.join("settings").unwrap()
    );
}

async fn pause_long_failing_connections(app: &AuthenticatedApp) -> (usize, usize) {
    app.app
        .integration_connection_service
        .read()
        .await
        .pause_long_failing_integration_connections(
            Utc::now() - TimeDelta::days(FAILING_THRESHOLD_DAYS),
        )
        .await
        .unwrap()
}

/// Mark the connection `Failing` since `failing_since`: through its syncs when
/// `by_syncs`, otherwise the way a failed token refresh does (no sync failure
/// timestamp, only its last update).
async fn set_failing_since(
    app: &AuthenticatedApp,
    integration_connection: &IntegrationConnection,
    failing_since: DateTime<Utc>,
    by_syncs: bool,
) {
    sqlx::query(
        r#"
            UPDATE integration_connection
            SET status = 'Failing',
                first_notifications_sync_failed_at = CASE WHEN $3 THEN $2 ELSE NULL END,
                updated_at = $2
            WHERE id = $1
        "#,
    )
    .bind(integration_connection.id.0)
    .bind(failing_since.naive_utc())
    .bind(by_syncs)
    .execute(&*app.app.repository.pool)
    .await
    .expect("Failed to mark the connection as failing");
}

async fn reload(
    app: &AuthenticatedApp,
    integration_connection: &IntegrationConnection,
) -> IntegrationConnection {
    get_integration_connection(app, integration_connection.id)
        .await
        .unwrap()
}

#[rstest]
#[case::just_past_the_threshold(TimeDelta::days(INACTIVITY_THRESHOLD_DAYS) + TimeDelta::hours(1), true)]
#[case::just_before_the_threshold(TimeDelta::days(INACTIVITY_THRESHOLD_DAYS) - TimeDelta::hours(1), false)]
#[tokio::test]
async fn test_slack_connection_is_paused_past_the_inactivity_threshold(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    #[case] inactive_for: TimeDelta,
    #[case] expect_paused: bool,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    set_last_active_at(&app.app, app.user.id, Utc::now() - inactive_for).await;
    let revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    run_pause_integration_connections(&app, NO_INACTIVITY_WARNING).await;

    let connection = reload(&app, &connection).await;
    if expect_paused {
        assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
        assert_eq!(
            connection.paused_reason,
            Some(IntegrationConnectionPausedReason::Inactivity)
        );
        assert!(connection.paused_at.is_some());
        assert!(!connection.is_connected());
        assert_eq!(revocation.received_requests().await.len(), 1);
        assert!(fetch_oauth_credential(&app, &connection).await.is_none());
        let emails = pause_emails(&app).await;
        assert_eq!(emails.len(), 1);
        assert_paused_email(
            &app,
            &emails[0],
            IntegrationConnectionPausedReason::Inactivity,
        );
    } else {
        assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
        assert_eq!(connection.paused_at, None);
        assert_eq!(revocation.received_requests().await.len(), 0);
        assert!(fetch_oauth_credential(&app, &connection).await.is_some());
        assert_eq!(pause_emails(&app).await, vec![]);
    }
}

/// Revoking an expired access token is not reliable with Slack token
/// rotation: it is refreshed first, and the fresh token is the one revoked.
#[rstest]
#[tokio::test]
async fn test_expired_access_token_is_refreshed_before_being_revoked(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, Some("xoxe-1-old-refresh-token")).await;
    expire_access_token(&app, &connection).await;
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS + 1),
    )
    .await;
    let refresh = Mock::given(method("POST"))
        .and(path("/oauth.v2.access"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains(
            "refresh_token=xoxe-1-old-refresh-token",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "access_token": "xoxe.xoxp-fresh-access-token",
            "refresh_token": "xoxe-1-fresh-refresh-token",
            "token_type": "user",
            "expires_in": 43200
        })))
        .mount_as_scoped(&app.app.slack_mock_server)
        .await;
    let revocation = mock_slack_revocation(&app, "xoxe.xoxp-fresh-access-token").await;

    assert_eq!(pause_inactive_users_connections(&app).await, (1, 0));

    assert_eq!(refresh.received_requests().await.len(), 1);
    assert_eq!(revocation.received_requests().await.len(), 1);
    let connection = reload(&app, &connection).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
    assert!(fetch_oauth_credential(&app, &connection).await.is_none());
}

/// When Slack refuses the revocation, the connection is paused all the same
/// and the token is queued for the retry-oauth-grant-revocations cron.
#[rstest]
#[tokio::test]
async fn test_failed_revocation_is_queued_for_retry(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS + 1),
    )
    .await;
    let revocation = Mock::given(method("POST"))
        .and(path("/auth.revoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": false, "error": "ratelimited"
        })))
        .mount_as_scoped(&app.app.slack_mock_server)
        .await;

    assert_eq!(pause_inactive_users_connections(&app).await, (1, 0));

    assert_eq!(revocation.received_requests().await.len(), 1);
    let connection = reload(&app, &connection).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
    assert!(fetch_oauth_credential(&app, &connection).await.is_none());
    let pending_revocations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM oauth_grant_revocation WHERE integration_connection_id = $1 AND status = 'Pending'",
    )
    .bind(connection.id.0)
    .fetch_one(&*app.app.repository.pool)
    .await
    .unwrap();
    assert_eq!(pending_revocations, 1);
}

/// A paused connection holds no credential and is not `Validated`: neither the
/// refresh-oauth-tokens cron nor a later pause run touches it again.
#[rstest]
#[tokio::test]
async fn test_paused_connection_is_skipped_by_later_runs_and_token_refresh(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    connect_slack(&app, &settings, Some("xoxe-1-refresh-token")).await;
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS + 1),
    )
    .await;
    let revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;
    assert_eq!(pause_inactive_users_connections(&app).await, (1, 0));

    assert_eq!(pause_inactive_users_connections(&app).await, (0, 0));
    assert_eq!(revocation.received_requests().await.len(), 1);

    let service = app.app.integration_connection_service.read().await;
    let mut transaction = service.begin().await.unwrap();
    let refreshed = service
        .refresh_expiring_tokens(&mut transaction, 60 * 24 * 365, None)
        .await
        .unwrap();
    assert_eq!(refreshed, (0, 0));
}

/// Reconnecting goes through the regular disconnect + OAuth flow, which
/// clears the pause.
#[rstest]
#[tokio::test]
async fn test_disconnecting_a_paused_connection_clears_the_pause(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS + 1),
    )
    .await;
    let _revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;
    assert_eq!(pause_inactive_users_connections(&app).await, (1, 0));

    let disconnected: Box<IntegrationConnection> = delete_resource(
        &app.client,
        &app.app.api_address,
        "integration-connections",
        connection.id.into(),
    )
    .await;

    assert_eq!(disconnected.status, IntegrationConnectionStatus::Created);
    assert_eq!(disconnected.paused_at, None);
    assert_eq!(disconnected.paused_reason, None);
}

/// Authenticated requests record the user's activity, at most once a day.
#[rstest]
#[tokio::test]
async fn test_authenticated_requests_record_user_activity_once_a_day(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let email: Pii<EmailAddress> = "inactive@example.com".parse().unwrap();
    let user = create_user(&app, email.clone(), "Very-harD-pasSword-5").await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    let login_response = login_user_response(&client, &app, email, "Very-harD-pasSword-5").await;
    assert_eq!(login_response.status(), http::StatusCode::OK);

    let long_ago = Utc::now() - TimeDelta::days(30);
    set_last_active_at(&app, user.id, long_ago).await;
    get_current_user(&client, &app).await;

    let last_active_at = fetch_last_active_at(&app, user.id).await;
    assert!(
        Utc::now() - last_active_at < TimeDelta::minutes(1),
        "an authenticated request should record the activity, got {last_active_at}"
    );

    // Recorded less than a day ago: further requests do not write again.
    set_last_active_at(&app, user.id, long_ago).await;
    get_current_user(&client, &app).await;

    assert_eq!(
        fetch_last_active_at(&app, user.id).await.timestamp(),
        long_ago.timestamp()
    );
}

#[rstest]
#[case::by_syncs_just_past_the_threshold(TimeDelta::hours(1), true, true)]
#[case::by_syncs_just_before_the_threshold(-TimeDelta::hours(1), true, false)]
#[case::by_token_refresh_past_the_threshold(TimeDelta::hours(1), false, true)]
#[tokio::test]
async fn test_slack_connection_is_paused_past_the_failing_threshold(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    #[case] past_threshold_by: TimeDelta,
    #[case] by_syncs: bool,
    #[case] expect_paused: bool,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    set_failing_since(
        &app,
        &connection,
        Utc::now() - TimeDelta::days(FAILING_THRESHOLD_DAYS) - past_threshold_by,
        by_syncs,
    )
    .await;
    let revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    run_pause_integration_connections(&app, NO_INACTIVITY_WARNING).await;

    let connection = reload(&app, &connection).await;
    if expect_paused {
        assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
        assert_eq!(
            connection.paused_reason,
            Some(IntegrationConnectionPausedReason::LongFailing)
        );
        assert_eq!(connection.failure_message, None);
        assert_eq!(revocation.received_requests().await.len(), 1);
        assert!(fetch_oauth_credential(&app, &connection).await.is_none());
        // Nothing the user did paused it: its notifications stay visible.
        assert!(!connection.should_set_aside_notifications());
        let emails = pause_emails(&app).await;
        assert_eq!(emails.len(), 1);
        assert_paused_email(
            &app,
            &emails[0],
            IntegrationConnectionPausedReason::LongFailing,
        );
    } else {
        assert_eq!(connection.status, IntegrationConnectionStatus::Failing);
        assert_eq!(revocation.received_requests().await.len(), 0);
        assert!(fetch_oauth_credential(&app, &connection).await.is_some());
    }
}

/// A connection that just turned `Failing` without a sync failure (token
/// refresh, Slack revocation event) is not paused because it was last updated
/// long ago: the status change dates it.
#[rstest]
#[tokio::test]
async fn test_old_connection_that_just_failed_is_not_paused(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    sqlx::query("UPDATE integration_connection SET updated_at = $2 WHERE id = $1")
        .bind(connection.id.0)
        .bind((Utc::now() - TimeDelta::days(FAILING_THRESHOLD_DAYS * 2)).naive_utc())
        .execute(&*app.app.repository.pool)
        .await
        .unwrap();
    let mut transaction = app.app.repository.begin().await.unwrap();
    app.app
        .repository
        .update_integration_connection_status(
            &mut transaction,
            connection.id,
            IntegrationConnectionStatus::Failing,
            Some("Refresh token rejected".to_string()),
            None,
            app.user.id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    run_pause_integration_connections(&app, NO_INACTIVITY_WARNING).await;

    let connection = reload(&app, &connection).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Failing);
    assert_eq!(revocation.received_requests().await.len(), 0);
}

/// Long failing connections of every OAuth provider are paused, not only
/// Slack ones.
#[rstest]
#[case::past_the_threshold(TimeDelta::hours(1), true)]
#[case::before_the_threshold(-TimeDelta::hours(1), false)]
#[tokio::test]
async fn test_linear_connection_is_paused_past_the_failing_threshold(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    #[case] past_threshold_by: TimeDelta,
    #[case] expect_paused: bool,
) {
    let app = authenticated_app.await;
    let connection = connect_linear(&app, &settings).await;
    set_failing_since(
        &app,
        &connection,
        Utc::now() - TimeDelta::days(FAILING_THRESHOLD_DAYS) - past_threshold_by,
        true,
    )
    .await;
    let revocation = mock_linear_revocation(&app).await;

    run_pause_integration_connections(&app, NO_INACTIVITY_WARNING).await;

    let connection = reload(&app, &connection).await;
    if expect_paused {
        assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
        assert_eq!(
            connection.paused_reason,
            Some(IntegrationConnectionPausedReason::LongFailing)
        );
        assert_eq!(revocation.received_requests().await.len(), 1);
        assert!(fetch_oauth_credential(&app, &connection).await.is_none());
        let emails = pause_emails(&app).await;
        assert_eq!(emails.len(), 1);
        assert_provider_paused_email(
            &app,
            &emails[0],
            &["Linear"],
            IntegrationConnectionPausedReason::LongFailing,
        );
    } else {
        assert_eq!(connection.status, IntegrationConnectionStatus::Failing);
        assert_eq!(revocation.received_requests().await.len(), 0);
        assert!(fetch_oauth_credential(&app, &connection).await.is_some());
    }
}

/// An inactive user is warned once about all their connections, of every
/// OAuth provider, then emailed once when they are all paused.
#[rstest]
#[tokio::test]
async fn test_inactive_user_gets_one_email_for_all_their_connections(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let slack_connection = connect_slack(&app, &settings, None).await;
    let linear_connection = connect_linear(&app, &settings).await;
    let slack_revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;
    let linear_revocation = mock_linear_revocation(&app).await;

    // Inactive past the warning threshold only: warned, not paused.
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now()
            - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS - INACTIVITY_WARNING_DAYS)
            - TimeDelta::hours(1),
    )
    .await;
    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;

    assert_eq!(
        pause_emails(&app).await,
        vec![EmailTemplate::IntegrationConnectionPauseWarning {
            first_name: app.user.first_name.clone(),
            provider_names: vec!["Linear".to_string(), "Slack".to_string()],
            inactive_for_days: INACTIVITY_THRESHOLD_DAYS - INACTIVITY_WARNING_DAYS,
            pause_date: (Utc::now() + TimeDelta::days(INACTIVITY_WARNING_DAYS)).date_naive(),
            app_url: app.app.front_base_url.clone(),
        }]
    );
    for connection in [&slack_connection, &linear_connection] {
        assert_eq!(
            reload(&app, connection).await.status,
            IntegrationConnectionStatus::Validated
        );
    }

    // Inactive past the pause threshold: both paused, one more email.
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS) - TimeDelta::hours(1),
    )
    .await;
    run_pause_integration_connections(&app, NO_INACTIVITY_WARNING).await;

    for connection in [&slack_connection, &linear_connection] {
        let connection = reload(&app, connection).await;
        assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
        assert_eq!(
            connection.paused_reason,
            Some(IntegrationConnectionPausedReason::Inactivity)
        );
        assert!(fetch_oauth_credential(&app, &connection).await.is_none());
    }
    assert_eq!(slack_revocation.received_requests().await.len(), 1);
    assert_eq!(linear_revocation.received_requests().await.len(), 1);
    let emails = pause_emails(&app).await;
    assert_eq!(emails.len(), 2);
    assert_provider_paused_email(
        &app,
        &emails[1],
        &["Linear", "Slack"],
        IntegrationConnectionPausedReason::Inactivity,
    );
}

/// A user whose connections of several providers keep failing is emailed
/// once when they are all paused.
#[rstest]
#[tokio::test]
async fn test_long_failing_connections_of_a_user_get_one_email(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let slack_connection = connect_slack(&app, &settings, None).await;
    let linear_connection = connect_linear(&app, &settings).await;
    for connection in [&slack_connection, &linear_connection] {
        set_failing_since(
            &app,
            connection,
            Utc::now() - TimeDelta::days(FAILING_THRESHOLD_DAYS + 1),
            true,
        )
        .await;
    }
    let _slack_revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;
    let _linear_revocation = mock_linear_revocation(&app).await;

    assert_eq!(pause_long_failing_connections(&app).await, (2, 0));

    let emails = pause_emails(&app).await;
    assert_eq!(emails.len(), 1);
    assert_provider_paused_email(
        &app,
        &emails[0],
        &["Linear", "Slack"],
        IntegrationConnectionPausedReason::LongFailing,
    );
}

/// A connection of an active user that syncs fine is never paused.
#[rstest]
#[tokio::test]
async fn test_healthy_connection_of_active_user_is_not_paused(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    let revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    assert_eq!(pause_inactive_users_connections(&app).await, (0, 0));
    assert_eq!(pause_long_failing_connections(&app).await, (0, 0));

    assert_eq!(revocation.received_requests().await.len(), 0);
    assert_eq!(
        reload(&app, &connection).await.status,
        IntegrationConnectionStatus::Validated
    );
}

/// A refresh token Slack rejects means the grant is already dead: nothing is
/// left to revoke, the connection is paused right away.
#[rstest]
#[tokio::test]
async fn test_dead_refresh_token_counts_as_revoked(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, Some("xoxe-1-dead-refresh-token")).await;
    expire_access_token(&app, &connection).await;
    set_failing_since(
        &app,
        &connection,
        Utc::now() - TimeDelta::days(FAILING_THRESHOLD_DAYS + 1),
        false,
    )
    .await;
    let _refresh = Mock::given(method("POST"))
        .and(path("/oauth.v2.access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": false, "error": "invalid_refresh_token"
        })))
        .mount_as_scoped(&app.app.slack_mock_server)
        .await;
    let revocation = Mock::given(method("POST"))
        .and(path("/auth.revoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount_as_scoped(&app.app.slack_mock_server)
        .await;

    assert_eq!(pause_long_failing_connections(&app).await, (1, 0));

    assert_eq!(revocation.received_requests().await.len(), 0);
    assert_eq!(
        reload(&app, &connection).await.status,
        IntegrationConnectionStatus::Paused
    );
}

/// Users inactive for `INACTIVITY_THRESHOLD_DAYS - INACTIVITY_WARNING_DAYS`
/// are warned once that their connection will be paused, and nothing else
/// happens yet.
#[rstest]
#[case::just_past_the_warning_threshold(TimeDelta::hours(1), true)]
#[case::just_before_the_warning_threshold(-TimeDelta::hours(1), false)]
#[tokio::test]
async fn test_inactive_user_is_warned_once_before_the_pause(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    #[case] past_warning_threshold_by: TimeDelta,
    #[case] expect_warned: bool,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now()
            - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS - INACTIVITY_WARNING_DAYS)
            - past_warning_threshold_by,
    )
    .await;
    let revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;
    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;

    let emails = pause_emails(&app).await;
    if expect_warned {
        assert_eq!(
            emails,
            vec![EmailTemplate::IntegrationConnectionPauseWarning {
                first_name: app.user.first_name.clone(),
                provider_names: vec!["Slack".to_string()],
                inactive_for_days: INACTIVITY_THRESHOLD_DAYS - INACTIVITY_WARNING_DAYS,
                pause_date: (Utc::now() + TimeDelta::days(INACTIVITY_WARNING_DAYS)).date_naive(),
                app_url: app.app.front_base_url.clone(),
            }]
        );
    } else {
        assert_eq!(emails, vec![]);
    }
    assert_eq!(
        reload(&app, &connection).await.status,
        IntegrationConnectionStatus::Validated
    );
    assert_eq!(revocation.received_requests().await.len(), 0);
}

/// A user already inactive past the pause threshold when first seen (e.g. the
/// cron was just enabled) is warned first, and paused only
/// `INACTIVITY_WARNING_DAYS` after the warning.
#[rstest]
#[tokio::test]
async fn test_inactive_user_is_paused_only_after_the_warning_period(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS + 10),
    )
    .await;
    let revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;

    assert_eq!(
        reload(&app, &connection).await.status,
        IntegrationConnectionStatus::Validated
    );
    let emails = pause_emails(&app).await;
    assert_eq!(emails.len(), 1);
    assert!(matches!(
        emails[0],
        EmailTemplate::IntegrationConnectionPauseWarning { .. }
    ));

    // Still within the warning period
    set_inactivity_warning_sent_at(
        &app,
        &connection,
        Utc::now() - TimeDelta::days(INACTIVITY_WARNING_DAYS) + TimeDelta::hours(1),
    )
    .await;
    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;
    assert_eq!(
        reload(&app, &connection).await.status,
        IntegrationConnectionStatus::Validated
    );
    assert_eq!(pause_emails(&app).await.len(), 1);

    // Past the warning period
    set_inactivity_warning_sent_at(
        &app,
        &connection,
        Utc::now() - TimeDelta::days(INACTIVITY_WARNING_DAYS) - TimeDelta::hours(1),
    )
    .await;
    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;

    let connection = reload(&app, &connection).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
    assert_eq!(revocation.received_requests().await.len(), 1);
    let emails = pause_emails(&app).await;
    assert_eq!(emails.len(), 2);
    assert_paused_email(
        &app,
        &emails[1],
        IntegrationConnectionPausedReason::Inactivity,
    );
}

/// A warning only counts for the inactivity period it was sent in: a user
/// active since is not paused on it, and is warned again once inactive again.
#[rstest]
#[tokio::test]
async fn test_warning_sent_before_the_last_activity_does_not_count(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    // Warned long ago, active since, and inactive past the threshold again
    set_inactivity_warning_sent_at(&app, &connection, Utc::now() - TimeDelta::days(200)).await;
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS + 10),
    )
    .await;
    let revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;

    assert_eq!(
        reload(&app, &connection).await.status,
        IntegrationConnectionStatus::Validated
    );
    assert_eq!(revocation.received_requests().await.len(), 0);
    let emails = pause_emails(&app).await;
    assert_eq!(emails.len(), 1);
    assert!(matches!(
        emails[0],
        EmailTemplate::IntegrationConnectionPauseWarning { .. }
    ));
}

/// Testing accounts are never emailed, but their connections are still
/// warned (silently) and paused on schedule.
#[rstest]
#[tokio::test]
async fn test_testing_account_is_paused_without_emails(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let connection = connect_slack(&app, &settings, None).await;
    sqlx::query(r#"UPDATE "user" SET is_testing = true WHERE id = $1"#)
        .bind(app.user.id.0)
        .execute(&*app.app.repository.pool)
        .await
        .unwrap();
    set_last_active_at(
        &app.app,
        app.user.id,
        Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS + 10),
    )
    .await;
    let _revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;
    assert_eq!(
        reload(&app, &connection).await.status,
        IntegrationConnectionStatus::Validated
    );

    set_inactivity_warning_sent_at(
        &app,
        &connection,
        Utc::now() - TimeDelta::days(INACTIVITY_WARNING_DAYS + 1),
    )
    .await;
    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;

    assert_eq!(
        reload(&app, &connection).await.status,
        IntegrationConnectionStatus::Paused
    );
    assert_eq!(pause_emails(&app).await, vec![]);
}

/// Before enabling the cron, `integration-connection pause-without-email`
/// pauses the connections it would otherwise email about.
async fn run_pause_without_email(
    app: &AuthenticatedApp,
    inactive_before: DateTime<Utc>,
    dry_run: bool,
) -> PauseWithoutEmailReport {
    pause_without_email(
        app.app.integration_connection_service.clone(),
        inactive_before,
        FAILING_THRESHOLD_DAYS,
        dry_run,
    )
    .await
    .unwrap()
}

/// Every connection of a user inactive since `inactive_before` is paused
/// without any email, even one failing for less than the failing threshold,
/// so that enabling the cron afterwards does not email them either.
#[rstest]
#[tokio::test]
async fn test_pause_without_email_pauses_the_connections_of_inactive_users(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let slack_connection = connect_slack(&app, &settings, None).await;
    let linear_connection = connect_linear(&app, &settings).await;
    set_failing_since(
        &app,
        &linear_connection,
        Utc::now() - TimeDelta::days(1),
        true,
    )
    .await;
    set_last_active_at(&app.app, app.user.id, Utc::now() - TimeDelta::days(30)).await;
    let slack_revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;
    let linear_revocation = mock_linear_revocation(&app).await;

    let report = run_pause_without_email(&app, Utc::now() - TimeDelta::days(1), false).await;

    assert_eq!(
        report,
        PauseWithoutEmailReport {
            paused_inactive: 1,
            paused_failing: 1,
            ..Default::default()
        }
    );
    let slack_connection = reload(&app, &slack_connection).await;
    assert_eq!(slack_connection.status, IntegrationConnectionStatus::Paused);
    assert_eq!(
        slack_connection.paused_reason,
        Some(IntegrationConnectionPausedReason::Inactivity)
    );
    assert!(slack_connection.should_set_aside_notifications());
    let linear_connection = reload(&app, &linear_connection).await;
    assert_eq!(
        linear_connection.status,
        IntegrationConnectionStatus::Paused
    );
    assert_eq!(
        linear_connection.paused_reason,
        Some(IntegrationConnectionPausedReason::LongFailing)
    );
    assert_eq!(slack_revocation.received_requests().await.len(), 1);
    assert_eq!(linear_revocation.received_requests().await.len(), 1);
    assert!(
        fetch_oauth_credential(&app, &slack_connection)
            .await
            .is_none()
    );
    assert!(
        fetch_oauth_credential(&app, &linear_connection)
            .await
            .is_none()
    );
    assert_eq!(pause_emails(&app).await, vec![]);

    // Enabling the cron afterwards neither warns nor emails them.
    set_last_active_at(&app.app, app.user.id, Utc::now() - TimeDelta::days(365)).await;
    run_pause_integration_connections(&app, INACTIVITY_WARNING_DAYS).await;
    assert_eq!(pause_emails(&app).await, vec![]);
}

/// The connections of active users are left alone, except those already
/// failing for more than the failing threshold.
#[rstest]
#[case::failing_past_the_threshold(TimeDelta::days(FAILING_THRESHOLD_DAYS + 1), true)]
#[case::failing_before_the_threshold(TimeDelta::days(FAILING_THRESHOLD_DAYS - 1), false)]
#[tokio::test]
async fn test_pause_without_email_pauses_only_long_failing_connections_of_active_users(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    #[case] failing_for: TimeDelta,
    #[case] expect_paused: bool,
) {
    let app = authenticated_app.await;
    let slack_connection = connect_slack(&app, &settings, None).await;
    let linear_connection = connect_linear(&app, &settings).await;
    set_failing_since(&app, &linear_connection, Utc::now() - failing_for, true).await;
    let slack_revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;
    let linear_revocation = mock_linear_revocation(&app).await;

    let report = run_pause_without_email(&app, Utc::now() - TimeDelta::days(1), false).await;

    assert_eq!(
        report,
        PauseWithoutEmailReport {
            paused_failing: usize::from(expect_paused),
            ..Default::default()
        }
    );
    assert_eq!(
        reload(&app, &slack_connection).await.status,
        IntegrationConnectionStatus::Validated
    );
    assert_eq!(slack_revocation.received_requests().await.len(), 0);
    let linear_connection = reload(&app, &linear_connection).await;
    if expect_paused {
        assert_eq!(
            linear_connection.status,
            IntegrationConnectionStatus::Paused
        );
        assert_eq!(
            linear_connection.paused_reason,
            Some(IntegrationConnectionPausedReason::LongFailing)
        );
        assert_eq!(linear_revocation.received_requests().await.len(), 1);
    } else {
        assert_eq!(
            linear_connection.status,
            IntegrationConnectionStatus::Failing
        );
        assert_eq!(linear_revocation.received_requests().await.len(), 0);
    }
    assert_eq!(pause_emails(&app).await, vec![]);
}

#[rstest]
#[tokio::test]
async fn test_pause_without_email_dry_run_changes_nothing(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let slack_connection = connect_slack(&app, &settings, None).await;
    set_last_active_at(&app.app, app.user.id, Utc::now() - TimeDelta::days(30)).await;
    let slack_revocation = mock_slack_revocation(&app, "slack_test_user_access_token").await;

    let report = run_pause_without_email(&app, Utc::now() - TimeDelta::days(1), true).await;

    assert_eq!(
        report,
        PauseWithoutEmailReport {
            paused_inactive: 1,
            ..Default::default()
        }
    );
    assert_eq!(
        reload(&app, &slack_connection).await.status,
        IntegrationConnectionStatus::Validated
    );
    assert_eq!(slack_revocation.received_requests().await.len(), 0);
    assert!(
        fetch_oauth_credential(&app, &slack_connection)
            .await
            .is_some()
    );
}

#[rstest]
#[tokio::test]
async fn test_pause_without_email_rejects_a_future_inactivity_date(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;

    let result = pause_without_email(
        app.app.integration_connection_service.clone(),
        Utc::now() + TimeDelta::days(1),
        FAILING_THRESHOLD_DAYS,
        false,
    )
    .await;

    assert!(result.is_err());
}
