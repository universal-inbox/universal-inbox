use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;

use universal_inbox::integration_connection::{
    IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
    config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
};
use universal_inbox_api::{
    commands::slack::{BackfillTeamIdReport, backfill_team_id},
    integrations::oauth2::AccessToken,
    repository::integration_connection::IntegrationConnectionRepository,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            create_and_mock_integration_connection, create_integration_connection, slack_context,
            slack_oauth_credential,
        },
        notification::slack::{mock_slack_auth_test, slack_auth_test_response},
        slack_backfill::expire_the_access_token,
    },
    world::ApiWorld,
};

const MY_TOKEN: &str = "slack_test_user_access_token";
const REVOKED_TOKEN: &str = "slack_revoked_access_token";

enum Mode {
    Apply,
    DryRun,
}

async fn connect(
    world: &mut ApiWorld,
    user_id: universal_inbox::user::UserId,
    credential: crate::helpers::integration_connection::OAuthCredentialFixture,
    context: Option<
        universal_inbox::integration_connection::provider::IntegrationConnectionContext,
    >,
) -> IntegrationConnectionId {
    let app = world.app.get().await;
    create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings(),
        credential,
        None,
        context,
    )
    .await
    .id
}

#[given("I have a Slack connection that has no team yet")]
async fn i_have_a_connection_without_team(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let id = connect(world, user_id, slack_oauth_credential(), None).await;
    world.slack_backfill.mine = Some(id);
}

#[given(expr = "I have a Slack connection in the team {string}")]
async fn i_have_a_connection_in_a_team(world: &mut ApiWorld, team: String) {
    let user_id = world.user().id;
    let id = connect(
        world,
        user_id,
        slack_oauth_credential(),
        Some(slack_context(&team)),
    )
    .await;
    world.slack_backfill.mine = Some(id);
}

#[given("I have a Slack connection that never completed its OAuth flow")]
async fn i_have_a_connection_without_credential(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let id = create_integration_connection(
        app,
        user_id,
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
    world.slack_backfill.mine = Some(id);
}

#[given(
    "I have a Slack connection whose access token expired an hour ago, and that has no team yet"
)]
async fn i_have_a_connection_with_an_expired_token(world: &mut ApiWorld) {
    i_have_a_connection_without_team(world).await;
    let id = world.slack_backfill.mine.unwrap();
    let app = world.app.get().await;
    expire_the_access_token(app, id).await;
}

#[given("another user has a Slack connection with a revoked access token, and no team yet")]
async fn another_user_has_a_connection_with_a_revoked_token(world: &mut ApiWorld) {
    let user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    let credential = crate::helpers::integration_connection::OAuthCredentialFixture {
        access_token: AccessToken(REVOKED_TOKEN.to_string()),
        ..slack_oauth_credential()
    };
    let id = connect(world, user_id, credential, None).await;
    world.slack_backfill.others = Some(id);
}

#[given(
    expr = "Slack tells that the access token of my connection belongs to the team {string}, and is asked once"
)]
async fn slack_knows_my_token(world: &mut ApiWorld, team: String) {
    let app = world.app.get().await;
    mock_slack_auth_test(
        &app.slack_mock_server,
        MY_TOKEN,
        slack_auth_test_response(&team),
        1,
    )
    .await;
}

#[given("Slack is not asked about the access token of my connection")]
async fn slack_is_not_asked_about_my_token(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_slack_auth_test(
        &app.slack_mock_server,
        MY_TOKEN,
        slack_auth_test_response("T053BSKET"),
        0,
    )
    .await;
}

#[given("Slack rejects the access token of that other user as invalid, and is asked once")]
async fn slack_rejects_the_revoked_token(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_slack_auth_test(
        &app.slack_mock_server,
        REVOKED_TOKEN,
        json!({ "ok": false, "error": "invalid_auth" }),
        1,
    )
    .await;
}

async fn run_backfill(world: &mut ApiWorld, mode: Mode) {
    let app = world.app.get().await;
    let report = backfill_team_id(
        app.integration_connection_service.clone(),
        app.slack_service.clone(),
        None,
        matches!(mode, Mode::DryRun),
    )
    .await
    .unwrap();
    world.slack_backfill.report = Some(report);
}

#[when("I run the team backfill")]
async fn i_run_the_backfill(world: &mut ApiWorld) {
    run_backfill(world, Mode::Apply).await;
}

#[when("I run the team backfill again")]
async fn i_run_the_backfill_again(world: &mut ApiWorld) {
    run_backfill(world, Mode::Apply).await;
}

#[when("I run the team backfill as a dry run")]
async fn i_run_the_backfill_as_a_dry_run(world: &mut ApiWorld) {
    run_backfill(world, Mode::DryRun).await;
}

#[then(
    expr = "the backfill report says {int} updated, {int} skipped without credential, {int} skipped without access token and {int} failed"
)]
async fn the_report_says(
    world: &mut ApiWorld,
    updated: usize,
    skipped_without_credential: usize,
    skipped_without_access_token: usize,
    failed: usize,
) {
    assert_eq!(
        world.slack_backfill.report.clone(),
        Some(BackfillTeamIdReport {
            updated,
            skipped_without_credential,
            skipped_without_access_token,
            failed,
        })
    );
}

async fn connection(world: &mut ApiWorld, id: IntegrationConnectionId) -> IntegrationConnection {
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connection = app
        .repository
        .get_integration_connection(&mut transaction, id)
        .await
        .unwrap()
        .expect("The connection should exist");
    transaction.commit().await.unwrap();
    connection
}

#[then(expr = "my Slack connection is in the team {string}")]
async fn my_connection_is_in_the_team(world: &mut ApiWorld, team: String) {
    let id = world
        .slack_backfill
        .mine
        .expect("I have no Slack connection");
    assert_eq!(
        connection(world, id).await.provider.context(),
        Some(slack_context(&team))
    );
}

#[then("my Slack connection has no team")]
async fn my_connection_has_no_team(world: &mut ApiWorld) {
    let id = world
        .slack_backfill
        .mine
        .expect("I have no Slack connection");
    assert_eq!(connection(world, id).await.provider.context(), None);
}

#[then("the Slack connection of that other user has no team")]
async fn the_other_connection_has_no_team(world: &mut ApiWorld) {
    let id = world
        .slack_backfill
        .others
        .expect("The other user has no Slack connection");
    assert_eq!(connection(world, id).await.provider.context(), None);
}

#[then("my Slack connection is still validated")]
async fn my_connection_is_still_validated(world: &mut ApiWorld) {
    let id = world
        .slack_backfill
        .mine
        .expect("I have no Slack connection");
    assert_eq!(
        connection(world, id).await.status,
        IntegrationConnectionStatus::Validated
    );
}
