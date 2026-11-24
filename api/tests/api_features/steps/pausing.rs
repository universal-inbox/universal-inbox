use chrono::{DateTime, TimeDelta, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionPausedReason, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{linear::LinearConfig, slack::SlackConfig},
        provider::IntegrationProviderKind,
    },
    user::UserId,
};
use universal_inbox_api::{
    commands::integration_connection::pause_without_email,
    integrations::oauth2::RefreshToken,
    jobs::oauth::pause_integration_connections,
    mailer::EmailTemplate,
    repository::{
        integration_connection::IntegrationConnectionRepository,
        oauth_credential::OAuthCredentialRepository,
    },
    universal_inbox::integration_connection::service::PauseWithoutEmailReport,
};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use crate::{
    common::settings,
    helpers::{
        TestedApp,
        integration_connection::{
            create_and_mock_integration_connection, linear_oauth_credential, slack_oauth_credential,
        },
        user::get_current_user,
    },
    steps::integration_connections::stored_connection,
    world::ApiWorld,
};

const INACTIVITY_THRESHOLD_DAYS: i64 = 90;
const FAILING_THRESHOLD_DAYS: i64 = 30;

/// "90 days", "90 days and 1 hour", "1 day"
fn duration(text: &str) -> TimeDelta {
    let mut parts = text.split(" and ");
    let days: i64 = parts
        .next()
        .and_then(|days| days.split(' ').next())
        .unwrap()
        .parse()
        .unwrap();
    let hours: i64 = parts
        .next()
        .map(|hours| hours.split(' ').next().unwrap().parse().unwrap())
        .unwrap_or(0);
    TimeDelta::days(days) + TimeDelta::hours(hours)
}

fn paused_reason(text: &str) -> IntegrationConnectionPausedReason {
    match text {
        "inactivity" => IntegrationConnectionPausedReason::Inactivity,
        "a long failure" => IntegrationConnectionPausedReason::LongFailing,
        other => panic!("Unknown pause reason {other:?}"),
    }
}

fn provider_names(text: &str) -> Vec<String> {
    text.split(" and ").map(str::to_string).collect()
}

async fn connect_slack(world: &mut ApiWorld, refresh_token: Option<&str>) {
    let user_id = world.user().id;
    let mut credential = slack_oauth_credential();
    credential.refresh_token = refresh_token.map(|token| RefreshToken(token.to_string()));
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings(),
        credential,
        None,
        None,
    )
    .await;
    world.connections.granted = Some(connection.clone());
    world.connections.slack = Some(connection);
}

#[given("I have a Slack connection syncing my notifications")]
async fn i_have_a_slack_connection(world: &mut ApiWorld) {
    connect_slack(world, None).await;
}

#[given(
    regex = r#"^I have a Slack connection syncing my notifications, with the refresh token "([^"]+)"$"#
)]
async fn i_have_a_slack_connection_with_a_refresh_token(
    world: &mut ApiWorld,
    refresh_token: String,
) {
    connect_slack(world, Some(&refresh_token)).await;
}

#[given("I have a Linear connection syncing my tasks")]
async fn i_have_a_linear_connection(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
        &settings(),
        linear_oauth_credential(),
        None,
        None,
    )
    .await;
    world.connections.linear = Some(connection);
}

async fn set_last_active_at(app: &TestedApp, user_id: UserId, last_active_at: DateTime<Utc>) {
    sqlx::query(r#"UPDATE "user" SET last_active_at = $2 WHERE id = $1"#)
        .bind(user_id.0)
        .bind(last_active_at.naive_utc())
        .execute(&*app.repository.pool)
        .await
        .expect("Failed to set the user last activity");
}

#[given(regex = r"^I was last active (\d+ days?(?: and \d+ hours?)?) ago$")]
#[when(regex = r"^I was last active (\d+ days?(?: and \d+ hours?)?) ago$")]
async fn i_was_last_active(world: &mut ApiWorld, ago: String) {
    let user_id = world.user().id;
    let last_active_at = Utc::now() - duration(&ago);
    world.connections.recorded_activity = Some(last_active_at);
    let app = world.app.get().await;
    set_last_active_at(app, user_id, last_active_at).await;
}

fn connection_of<'a>(world: &'a ApiWorld, provider: &str) -> &'a IntegrationConnection {
    match provider {
        "Slack" => world.connections.slack.as_deref(),
        "Linear" => world.connections.linear.as_deref(),
        other => panic!("No {other} connection in this scenario"),
    }
    .unwrap_or_else(|| panic!("No {provider} connection in this scenario"))
}

async fn set_failing_since(
    world: &mut ApiWorld,
    provider: &str,
    failing_for: TimeDelta,
    by_syncs: bool,
) {
    let id = connection_of(world, provider).id;
    let failing_since = Utc::now() - failing_for;
    let app = world.app.get().await;
    sqlx::query(
        r#"
            UPDATE integration_connection
            SET status = 'Failing',
                first_notifications_sync_failed_at = CASE WHEN $3 THEN $2 ELSE NULL END,
                updated_at = $2
            WHERE id = $1
        "#,
    )
    .bind(id.0)
    .bind(failing_since.naive_utc())
    .bind(by_syncs)
    .execute(&*app.repository.pool)
    .await
    .expect("Failed to mark the connection as failing");
}

#[given(
    regex = r"^my (Slack|Linear) connection has been failing for (\d+ days?(?: and \d+ hours?)?), through its syncs$"
)]
async fn my_connection_has_been_failing(
    world: &mut ApiWorld,
    provider: String,
    failing_for: String,
) {
    set_failing_since(world, &provider, duration(&failing_for), true).await;
}

#[given(
    regex = r"^my Slack connection has been failing for (\d+ days?(?: and \d+ hours?)?), since a token refresh failed$"
)]
async fn my_connection_has_been_failing_since_a_refresh(world: &mut ApiWorld, failing_for: String) {
    set_failing_since(world, "Slack", duration(&failing_for), false).await;
}

#[given(regex = r"^my Slack connection was last updated (\d+ days?(?: and \d+ hours?)?) ago$")]
async fn my_connection_was_last_updated(world: &mut ApiWorld, ago: String) {
    let id = connection_of(world, "Slack").id;
    let updated_at = Utc::now() - duration(&ago);
    let app = world.app.get().await;
    sqlx::query("UPDATE integration_connection SET updated_at = $2 WHERE id = $1")
        .bind(id.0)
        .bind(updated_at.naive_utc())
        .execute(&*app.repository.pool)
        .await
        .unwrap();
}

#[given(regex = r"^the warning I received is (\d+ days?(?: and \d+ hours?)?) old$")]
#[when(regex = r"^the warning I received is (\d+ days?(?: and \d+ hours?)?) old$")]
async fn the_warning_was_sent(world: &mut ApiWorld, ago: String) {
    let id = connection_of(world, "Slack").id;
    let sent_at = Utc::now() - duration(&ago);
    let app = world.app.get().await;
    sqlx::query("UPDATE integration_connection SET inactivity_warning_sent_at = $2 WHERE id = $1")
        .bind(id.0)
        .bind(sent_at.naive_utc())
        .execute(&*app.repository.pool)
        .await
        .expect("Failed to set the inactivity warning time");
}

#[given("my account is a testing account")]
async fn my_account_is_a_testing_account(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    sqlx::query(r#"UPDATE "user" SET is_testing = true WHERE id = $1"#)
        .bind(user_id.0)
        .execute(&*app.repository.pool)
        .await
        .unwrap();
}

#[given("Slack answers that revoking is rate limited")]
async fn slack_is_rate_limited(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let guard = Mock::given(method("POST"))
        .and(path("/auth.revoke"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "ok": false, "error": "ratelimited" })),
        )
        .mount_as_scoped(&app.slack_mock_server)
        .await;
    world
        .connections
        .calls
        .revocations
        .insert(IntegrationProviderKind::Slack, guard);
}

#[given("Slack rejects my refresh token")]
async fn slack_rejects_the_refresh_token(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let guard = Mock::given(method("POST"))
        .and(path("/oauth.v2.access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": false, "error": "invalid_refresh_token"
        })))
        .mount_as_scoped(&app.slack_mock_server)
        .await;
    world.connections.calls.renewal = Some(guard);
}

#[given("Slack accepts any revocation")]
async fn slack_accepts_any_revocation(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let guard = Mock::given(method("POST"))
        .and(path("/auth.revoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .mount_as_scoped(&app.slack_mock_server)
        .await;
    world
        .connections
        .calls
        .revocations
        .insert(IntegrationProviderKind::Slack, guard);
}

async fn run_pause_job(world: &mut ApiWorld, warning_days: i64) {
    let app = world.app.get().await;
    pause_integration_connections(
        app.integration_connection_service.clone(),
        INACTIVITY_THRESHOLD_DAYS,
        warning_days,
        FAILING_THRESHOLD_DAYS,
    )
    .await
    .unwrap();
}

#[when("the pause job runs")]
async fn the_pause_job_runs(world: &mut ApiWorld) {
    run_pause_job(world, 0).await;
}

#[when(expr = "the pause job runs, warning inactive users {int} days ahead")]
async fn the_pause_job_runs_with_warnings(world: &mut ApiWorld, warning_days: i64) {
    run_pause_job(world, warning_days).await;
}

#[when("only the connections of inactive users are paused")]
async fn only_inactive_connections_are_paused(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let result = app
        .integration_connection_service
        .read()
        .await
        .pause_integration_connections_of_inactive_users(
            Utc::now() - TimeDelta::days(INACTIVITY_THRESHOLD_DAYS),
            None,
        )
        .await
        .unwrap();
    world.connections.reported = Some(result);
}

#[when("only the long-failing connections are paused")]
async fn only_failing_connections_are_paused(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let result = app
        .integration_connection_service
        .read()
        .await
        .pause_long_failing_integration_connections(
            Utc::now() - TimeDelta::days(FAILING_THRESHOLD_DAYS),
        )
        .await
        .unwrap();
    world.connections.reported = Some(result);
}

#[when("the tokens expiring within a year are refreshed")]
async fn the_expiring_tokens_are_refreshed(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let service = app.integration_connection_service.read().await;
    let mut transaction = service.begin().await.unwrap();
    let refreshed = service
        .refresh_expiring_tokens(&mut transaction, 60 * 24 * 365, None)
        .await
        .unwrap();
    world.connections.reported = Some(refreshed);
}

#[when(regex = r#"^my Slack connection turns failing with the message "([^"]+)"$"#)]
async fn my_connection_turns_failing(world: &mut ApiWorld, message: String) {
    let id = connection_of(world, "Slack").id;
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_integration_connection_status(
            &mut transaction,
            id,
            IntegrationConnectionStatus::Failing,
            Some(message),
            None,
            user_id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn pause_without_email_of_users_inactive_since_yesterday(
    world: &mut ApiWorld,
    dry_run: bool,
) {
    let app = world.app.get().await;
    let report = pause_without_email(
        app.integration_connection_service.clone(),
        Utc::now() - TimeDelta::days(1),
        FAILING_THRESHOLD_DAYS,
        dry_run,
    )
    .await
    .unwrap();
    world.connections.report = Some(report);
}

#[when("the connections of users inactive since yesterday are paused without email")]
async fn paused_without_email(world: &mut ApiWorld) {
    pause_without_email_of_users_inactive_since_yesterday(world, false).await;
}

#[when(
    "the connections of users inactive since yesterday are checked for a pause without email, as a dry run"
)]
async fn paused_without_email_dry_run(world: &mut ApiWorld) {
    pause_without_email_of_users_inactive_since_yesterday(world, true).await;
}

#[when("the connections of users inactive since tomorrow are paused without email")]
async fn paused_without_email_since_tomorrow(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let result = pause_without_email(
        app.integration_connection_service.clone(),
        Utc::now() + TimeDelta::days(1),
        FAILING_THRESHOLD_DAYS,
        false,
    )
    .await;
    world.connections.refused = Some(result.is_err());
}

#[when("I open my account page")]
async fn i_open_my_account_page(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    get_current_user(&client, app).await;
}

#[then(regex = r"^the pause job reports (\d+) connections paused and (\d+) failures$")]
fn the_pause_reported(world: &mut ApiWorld, paused: usize, failed: usize) {
    assert_eq!(world.connections.reported, Some((paused, failed)));
}

#[then(regex = r"^the report counts (\d+) inactive and (\d+) failing connections paused$")]
fn the_report_counts(world: &mut ApiWorld, inactive: usize, failing: usize) {
    assert_eq!(
        world.connections.report,
        Some(PauseWithoutEmailReport {
            paused_inactive: inactive,
            paused_failing: failing,
            ..Default::default()
        })
    );
}

#[then("the request is refused")]
fn the_request_is_refused(world: &mut ApiWorld) {
    assert_eq!(world.connections.refused, Some(true));
}

#[then(regex = r"^my (Slack|Linear) connection is paused for (inactivity|a long failure)$")]
async fn my_connection_is_paused(world: &mut ApiWorld, provider: String, reason: String) {
    let id = connection_of(world, &provider).id;
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
    assert_eq!(connection.paused_reason, Some(paused_reason(&reason)));
    assert!(connection.paused_at.is_some());
    assert_eq!(connection.failure_message, None);
    assert!(!connection.is_connected());
}

#[then(regex = r"^my (Slack|Linear) connection is still validated, and was never paused$")]
async fn my_connection_is_still_validated(world: &mut ApiWorld, provider: String) {
    let id = connection_of(world, &provider).id;
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert_eq!(connection.paused_at, None);
}

#[then(regex = r"^my (Slack|Linear) connection is still failing$")]
async fn my_connection_is_still_failing(world: &mut ApiWorld, provider: String) {
    let id = connection_of(world, &provider).id;
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Failing);
}

#[then("my Slack connection is paused")]
async fn my_slack_connection_is_paused(world: &mut ApiWorld) {
    let id = connection_of(world, "Slack").id;
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Paused);
}

#[then("my Slack connection keeps its notifications in my inbox")]
async fn my_connection_keeps_its_notifications(world: &mut ApiWorld) {
    let id = connection_of(world, "Slack").id;
    let connection = stored_connection(world, id).await;
    assert!(!connection.should_set_aside_notifications());
}

#[then("my Slack connection has its notifications set aside")]
async fn my_connection_sets_its_notifications_aside(world: &mut ApiWorld) {
    let id = connection_of(world, "Slack").id;
    let connection = stored_connection(world, id).await;
    assert!(connection.should_set_aside_notifications());
}

async fn holds_a_credential(world: &mut ApiWorld, provider: &str) -> bool {
    let id = connection_of(world, provider).id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let credential = app
        .repository
        .get_oauth_credential(&mut transaction, id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    credential.is_some()
}

#[then(regex = r"^my (Slack|Linear) connection still holds its credential$")]
async fn my_connection_holds_its_credential(world: &mut ApiWorld, provider: String) {
    assert!(holds_a_credential(world, &provider).await);
}

#[then(regex = r"^my (Slack|Linear) connection no longer holds a credential$")]
async fn my_connection_holds_no_credential(world: &mut ApiWorld, provider: String) {
    assert!(!holds_a_credential(world, &provider).await);
}

#[then("one revocation of my Slack grant is waiting to be retried")]
async fn a_revocation_is_waiting(world: &mut ApiWorld) {
    let id = connection_of(world, "Slack").id;
    let app = world.app.get().await;
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM oauth_grant_revocation WHERE integration_connection_id = $1 AND status = 'Pending'",
    )
    .bind(id.0)
    .fetch_one(&*app.repository.pool)
    .await
    .unwrap();
    assert_eq!(pending, 1);
}

#[then("it is no longer paused")]
fn it_is_no_longer_paused(world: &mut ApiWorld) {
    let connection = world.connections.disconnected.as_ref().unwrap();
    assert_eq!(connection.paused_at, None);
    assert_eq!(connection.paused_reason, None);
}

async fn pause_emails(world: &mut ApiWorld) -> Vec<EmailTemplate> {
    let app = world.app.get().await;
    app.mailer_stub
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

#[then("I got no pause email")]
async fn i_got_no_pause_email(world: &mut ApiWorld) {
    assert_eq!(pause_emails(world).await, vec![]);
}

#[then(regex = r"^I got (\d+) pause emails?$")]
async fn i_got_pause_emails(world: &mut ApiWorld, count: usize) {
    assert_eq!(pause_emails(world).await.len(), count);
}

#[then("my first pause email is a warning")]
async fn my_first_pause_email_is_a_warning(world: &mut ApiWorld) {
    let emails = pause_emails(world).await;
    assert!(
        matches!(
            emails.first(),
            Some(EmailTemplate::IntegrationConnectionPauseWarning { .. })
        ),
        "{emails:?}"
    );
}

#[then(
    regex = r"^I was warned once that my (.+?) connections? would be paused, after (\d+) days of inactivity$"
)]
async fn i_was_warned_once(world: &mut ApiWorld, providers: String, inactive_days: i64) {
    let first_name = world.user().first_name.clone();
    let app = world.app.get().await;
    let app_url = app.front_base_url.clone();
    let warning_days = INACTIVITY_THRESHOLD_DAYS - inactive_days;
    assert_eq!(
        pause_emails(world).await,
        vec![EmailTemplate::IntegrationConnectionPauseWarning {
            first_name,
            provider_names: provider_names(&providers),
            inactive_for_days: inactive_days,
            pause_date: (Utc::now() + TimeDelta::days(warning_days)).date_naive(),
            app_url,
        }]
    );
}

#[then(
    regex = r"^my last pause email says that my (.+?) connections? (?:was|were) paused for (inactivity|a long failure)$"
)]
async fn my_last_pause_email_says(world: &mut ApiWorld, providers: String, reason: String) {
    let emails = pause_emails(world).await;
    let app = world.app.get().await;
    let reconnect_url = app.front_base_url.join("settings").unwrap();
    let Some(EmailTemplate::IntegrationConnectionPaused {
        provider_names: names,
        paused_reason: paused,
        reconnect_url: url,
        ..
    }) = emails.last()
    else {
        panic!("Expected a paused email, got {emails:?}");
    };
    assert_eq!(*names, provider_names(&providers));
    assert_eq!(*paused, paused_reason(&reason));
    assert_eq!(*url, reconnect_url);
}

async fn last_active_at(world: &mut ApiWorld) -> DateTime<Utc> {
    let user_id: Uuid = world.user().id.0;
    let app = world.app.get().await;
    sqlx::query_scalar::<_, chrono::NaiveDateTime>(
        r#"SELECT last_active_at FROM "user" WHERE id = $1"#,
    )
    .bind(user_id)
    .fetch_one(&*app.repository.pool)
    .await
    .map(|last_active_at| last_active_at.and_utc())
    .expect("Failed to fetch the user last activity")
}

#[then("my last activity is recorded as just now")]
async fn my_last_activity_is_just_now(world: &mut ApiWorld) {
    let recorded = last_active_at(world).await;
    assert!(
        Utc::now() - recorded < TimeDelta::minutes(1),
        "an authenticated request should record the activity, got {recorded}"
    );
}

#[then("my last activity is still the one recorded before")]
async fn my_last_activity_is_unchanged(world: &mut ApiWorld) {
    let before = world.connections.recorded_activity.unwrap();
    assert_eq!(last_active_at(world).await.timestamp(), before.timestamp());
}
