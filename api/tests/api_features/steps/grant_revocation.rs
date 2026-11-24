use cucumber::{gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;
use uuid::Uuid;
use wiremock::{
    Mock, MockGuard, MockServer, ResponseTemplate,
    matchers::{basic_auth, body_json, body_string_contains, header, method, path},
};

use universal_inbox::integration_connection::{
    IntegrationConnection, IntegrationConnectionStatus,
    config::IntegrationConnectionConfig,
    integrations::{
        github::GithubConfig, google_mail::GoogleMailConfig, linear::LinearConfig,
        notion::NotionConfig, slack::SlackConfig, ticktick::TickTickConfig,
    },
    provider::IntegrationProviderKind,
};
use universal_inbox_api::{
    configuration::Settings, integrations::oauth2::RefreshToken,
    repository::oauth_credential::OAuthCredentialRepository,
    universal_inbox::integration_connection::service::GrantRevocationRetryPolicy,
};

use crate::{
    common::settings,
    helpers::{
        TestedApp,
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            github_oauth_credential, google_mail_oauth_credential, linear_oauth_credential,
            notion_oauth_credential, slack_oauth_credential, ticktick_oauth_credential,
        },
    },
    steps::provider::Provider,
    world::ApiWorld,
};

fn mock_server(app: &TestedApp, kind: IntegrationProviderKind) -> &MockServer {
    match kind {
        IntegrationProviderKind::Github => &app.github_mock_server,
        IntegrationProviderKind::GoogleMail => &app.google_mail_mock_server,
        IntegrationProviderKind::Slack => &app.slack_mock_server,
        IntegrationProviderKind::Linear => &app.linear_mock_server,
        IntegrationProviderKind::TickTick => &app.ticktick_mock_server,
        IntegrationProviderKind::Notion => &app.notion_mock_server,
        other => panic!("{other:?} grants are not revoked"),
    }
}

fn connection_kit(
    kind: IntegrationProviderKind,
) -> (IntegrationConnectionConfig, OAuthCredentialFixture) {
    match kind {
        IntegrationProviderKind::Github => (
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            github_oauth_credential(),
        ),
        IntegrationProviderKind::GoogleMail => (
            IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
            google_mail_oauth_credential(),
        ),
        IntegrationProviderKind::Slack => (
            IntegrationConnectionConfig::Slack(SlackConfig::default()),
            slack_oauth_credential(),
        ),
        IntegrationProviderKind::Linear => (
            IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
            linear_oauth_credential(),
        ),
        IntegrationProviderKind::TickTick => (
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            ticktick_oauth_credential(),
        ),
        IntegrationProviderKind::Notion => (
            IntegrationConnectionConfig::Notion(NotionConfig::enabled()),
            notion_oauth_credential(),
        ),
        other => panic!("{other:?} grants are not revoked"),
    }
}

async fn connect(world: &mut ApiWorld, kind: IntegrationProviderKind, rotating: bool) {
    let user_id = world.user().id;
    let (config, mut credential) = connection_kit(kind);
    if rotating {
        credential.refresh_token = Some(RefreshToken("xoxe-1-old-refresh-token".to_string()));
    }
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        config,
        &settings(),
        credential,
        None,
        None,
    )
    .await;
    world.connections.granted = Some(connection);
}

#[given(
    regex = r"^I have a validated (GitHub|Google Mail|Slack|Linear|TickTick|Notion) connection with a stored credential$"
)]
async fn i_have_a_connection_with_a_credential(world: &mut ApiWorld, provider: String) {
    connect(world, provider_kind(&provider), false).await;
}

fn provider_kind(name: &str) -> IntegrationProviderKind {
    name.parse::<Provider>().unwrap().0
}

#[given("I have a validated Slack connection whose credential rotates")]
async fn i_have_a_rotating_slack_connection(world: &mut ApiWorld) {
    connect(world, IntegrationProviderKind::Slack, true).await;
}

#[given("its access token expired an hour ago")]
async fn its_access_token_expired(world: &mut ApiWorld) {
    let id = world.connections.granted.as_ref().unwrap().id;
    let app = world.app.get().await;
    sqlx::query(
        "UPDATE oauth_credential SET access_token_expires_at = NOW() - INTERVAL '1 hour' \
         WHERE integration_connection_id = $1",
    )
    .bind(id.0)
    .execute(&*app.repository.pool)
    .await
    .unwrap();
}

async fn slack_revocation(
    world: &mut ApiWorld,
    access_token: &str,
    response: serde_json::Value,
) -> MockGuard {
    let app = world.app.get().await;
    Mock::given(method("POST"))
        .and(path("/auth.revoke"))
        .and(header(
            "authorization",
            format!("Bearer {access_token}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount_as_scoped(&app.slack_mock_server)
        .await
}

async fn accept_revocation(
    app: &TestedApp,
    settings: &Settings,
    kind: IntegrationProviderKind,
) -> MockGuard {
    let server = mock_server(app, kind);
    match kind {
        IntegrationProviderKind::Github => {
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
        IntegrationProviderKind::GoogleMail => {
            Mock::given(method("POST"))
                .and(path("/revoke"))
                .and(body_string_contains("token=google_mail_test_refresh_token"))
                .and(body_string_contains("token_type_hint=refresh_token"))
                .respond_with(ResponseTemplate::new(200))
                .mount_as_scoped(server)
                .await
        }
        IntegrationProviderKind::Slack => {
            Mock::given(method("POST"))
                .and(path("/auth.revoke"))
                .and(header(
                    "authorization",
                    "Bearer slack_test_user_access_token",
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({ "ok": true, "revoked": true })),
                )
                .mount_as_scoped(server)
                .await
        }
        IntegrationProviderKind::Linear => {
            Mock::given(method("POST"))
                .and(path("/oauth/revoke"))
                .and(body_string_contains("token=linear_test_refresh_token"))
                .respond_with(ResponseTemplate::new(200))
                .mount_as_scoped(server)
                .await
        }
        IntegrationProviderKind::TickTick => {
            let ticktick_settings = &settings.integrations["ticktick"];
            Mock::given(method("POST"))
                .and(path("/oauth/revoke"))
                .and(basic_auth(
                    &ticktick_settings.oauth_client_id,
                    ticktick_settings.oauth_client_secret.as_str(),
                ))
                .and(body_string_contains("token=ticktick_test_access_token"))
                .respond_with(ResponseTemplate::new(200))
                .mount_as_scoped(server)
                .await
        }
        IntegrationProviderKind::Notion => {
            let notion_settings = &settings.integrations["notion"];
            Mock::given(method("POST"))
                .and(path("/v1/oauth/revoke"))
                .and(basic_auth(
                    &notion_settings.oauth_client_id,
                    notion_settings.oauth_client_secret.as_str(),
                ))
                .and(body_json(json!({ "token": "notion_test_access_token" })))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
                .mount_as_scoped(server)
                .await
        }
        other => panic!("{other:?} grants are not revoked"),
    }
}

#[given(regex = r"^(GitHub|Google Mail|Slack|Linear|TickTick|Notion) revokes my grant when asked$")]
#[when(regex = r"^(GitHub|Google Mail|Slack|Linear|TickTick|Notion) revokes my grant when asked$")]
async fn the_provider_accepts_to_revoke(world: &mut ApiWorld, provider: String) {
    let app = world.app.get().await;
    let guard = accept_revocation(app, &settings(), provider_kind(&provider)).await;
    world
        .connections
        .guards
        .push((format!("{provider} grant"), guard));
}

#[given("GitHub fails to revoke my grant")]
async fn github_fails_to_revoke(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let settings = settings();
    let client_id = &settings.integrations["github"].oauth_client_id;
    let guard = Mock::given(method("DELETE"))
        .and(path(format!("/applications/{client_id}/grant")))
        .respond_with(ResponseTemplate::new(500))
        .mount_as_scoped(&app.github_mock_server)
        .await;
    world
        .connections
        .guards
        .push(("failure".to_string(), guard));
}

#[given("Slack is unavailable when asked to revoke a token")]
async fn slack_is_unavailable(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let guard = Mock::given(method("POST"))
        .and(path("/auth.revoke"))
        .respond_with(ResponseTemplate::new(503))
        .mount_as_scoped(&app.slack_mock_server)
        .await;
    world
        .connections
        .guards
        .push(("failure".to_string(), guard));
}

#[given(regex = r"^(GitHub|Slack) is back$")]
#[when(regex = r"^(GitHub|Slack) is back$")]
fn the_provider_is_back(world: &mut ApiWorld, _provider: String) {
    world.connections.release("failure");
}

#[given("Slack answers that my token expired")]
#[when("Slack answers that my token expired")]
async fn slack_answers_token_expired(world: &mut ApiWorld) {
    let guard = slack_revocation(
        world,
        "slack_test_user_access_token",
        json!({ "ok": false, "error": "token_expired" }),
    )
    .await;
    world
        .connections
        .guards
        .push(("expired".to_string(), guard));
}

#[given("Slack answers that my token was already revoked")]
async fn slack_answers_token_revoked(world: &mut ApiWorld) {
    let guard = slack_revocation(
        world,
        "slack_test_user_access_token",
        json!({ "ok": false, "error": "token_revoked" }),
    )
    .await;
    world
        .connections
        .guards
        .push(("Slack grant".to_string(), guard));
}

#[given("Slack renews my token when asked")]
#[when("Slack renews my token when asked")]
async fn slack_renews_my_token(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let guard = Mock::given(method("POST"))
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
        .mount_as_scoped(&app.slack_mock_server)
        .await;
    world
        .connections
        .guards
        .push(("refresh".to_string(), guard));
}

#[given("Slack accepts to revoke my renewed token")]
#[when("Slack accepts to revoke my renewed token")]
async fn slack_accepts_the_renewed_token(world: &mut ApiWorld) {
    let guard = slack_revocation(
        world,
        "xoxe.xoxp-new-access-token",
        json!({ "ok": true, "revoked": true }),
    )
    .await;
    world
        .connections
        .guards
        .push(("Slack grant".to_string(), guard));
}

#[when("I disconnect it")]
async fn i_disconnect_it(world: &mut ApiWorld) {
    let id = world.connections.granted.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let disconnected: Box<IntegrationConnection> = crate::helpers::rest::delete_resource(
        &client,
        &app.api_address,
        "integration-connections",
        id.into(),
    )
    .await;
    assert_eq!(disconnected.status, IntegrationConnectionStatus::Created);
    world.connections.disconnected = Some(*disconnected);
}

#[when("I connect it again")]
async fn i_connect_it_again(world: &mut ApiWorld) {
    let id = world.connections.granted.as_ref().unwrap().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .store_oauth_credential(
            &mut transaction,
            id,
            b"new encrypted token".to_vec(),
            None,
            None,
            json!({}),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[given("the waiting revocations come due")]
#[when("the waiting revocations come due")]
async fn the_waiting_revocations_come_due(world: &mut ApiWorld) {
    let app = world.app.get().await;
    sqlx::query("UPDATE oauth_grant_revocation SET next_attempt_at = NOW()")
        .execute(&*app.repository.pool)
        .await
        .unwrap();
}

#[when(expr = "the revocation retry job runs, allowing {int} attempts")]
async fn the_retry_job_runs(world: &mut ApiWorld, max_attempts: u32) {
    let app = world.app.get().await;
    let result = app
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
        .unwrap();
    world.connections.retried = Some(result);
}

async fn requests_received(world: &ApiWorld, name: &str) -> usize {
    world
        .connections
        .guard(name)
        .received_requests()
        .await
        .len()
}

#[then(
    regex = r"^(GitHub|Google Mail|Slack|Linear|TickTick|Notion) was asked once to revoke my grant$"
)]
async fn the_provider_was_asked_once(world: &mut ApiWorld, provider: String) {
    assert_eq!(
        requests_received(world, &format!("{provider} grant")).await,
        1,
        "the {provider} grant should have been revoked"
    );
}

#[then(
    regex = r"^(GitHub|Google Mail|Slack|Linear|TickTick|Notion) was not asked to revoke my grant$"
)]
async fn the_provider_was_not_asked(world: &mut ApiWorld, provider: String) {
    assert_eq!(
        requests_received(world, &format!("{provider} grant")).await,
        0,
        "the {provider} grant must not be revoked"
    );
}

#[then(regex = r"^(GitHub|Slack) was asked once to revoke my grant, and failed$")]
async fn the_provider_failed_once(world: &mut ApiWorld, provider: String) {
    assert_eq!(
        requests_received(world, "failure").await,
        1,
        "{provider} should have been asked once"
    );
}

#[then("Slack was asked once to renew my token")]
async fn slack_was_asked_to_renew(world: &mut ApiWorld) {
    assert_eq!(requests_received(world, "refresh").await, 1);
}

#[then("Slack was asked once about my expired token")]
async fn slack_was_asked_about_the_expired_token(world: &mut ApiWorld) {
    assert_eq!(requests_received(world, "expired").await, 1);
}

#[then(expr = "the revocation retry job completed {int} and failed {int}")]
fn the_retry_job_reports(world: &mut ApiWorld, completed: usize, failed: usize) {
    assert_eq!(world.connections.retried, Some((completed, failed)));
}

#[then("no revocation is stored")]
async fn no_revocation_is_stored(world: &mut ApiWorld) {
    assert_eq!(stored_revocations(world).await, vec![]);
}

#[derive(Debug, PartialEq)]
struct Revocation {
    status: String,
    attempts: i32,
    tokens: String,
    connection: String,
}

async fn stored_revocations(world: &mut ApiWorld) -> Vec<Revocation> {
    let granted = world.connections.granted.as_ref().map(|c| c.id.0);
    let app = world.app.get().await;
    sqlx::query_as::<_, (String, i32, bool, Option<Uuid>)>(
        "SELECT status::TEXT, attempts, encrypted_access_token IS NOT NULL, integration_connection_id \
         FROM oauth_grant_revocation ORDER BY created_at",
    )
    .fetch_all(&*app.repository.pool)
    .await
    .unwrap()
    .into_iter()
    .map(|(status, attempts, has_tokens, connection_id)| Revocation {
        status,
        attempts,
        tokens: if has_tokens { "kept" } else { "erased" }.to_string(),
        connection: match connection_id {
            None => "none".to_string(),
            Some(id) if Some(id) == granted => "mine".to_string(),
            Some(id) => format!("{id}"),
        },
    })
    .collect()
}

#[then("the stored revocations are")]
async fn the_stored_revocations_are(world: &mut ApiWorld, step: &Step) {
    let table = step.table.as_ref().expect("a table of revocations");
    let expected: Vec<Revocation> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| Revocation {
            status: row[0].clone(),
            attempts: row[1].parse().unwrap(),
            tokens: row[2].clone(),
            connection: row[3].clone(),
        })
        .collect();
    assert_eq!(stored_revocations(world).await, expected);
}
