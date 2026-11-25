use chrono::{TimeDelta, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
        provider::IntegrationConnectionContext,
    },
    user::UserId,
};
use universal_inbox_api::{
    commands::slack::{BackfillTeamIdReport, backfill_team_id},
    integrations::oauth2::AccessToken,
    repository::{
        integration_connection::IntegrationConnectionRepository,
        oauth_credential::OAuthCredentialRepository,
    },
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            create_integration_connection, slack_context, slack_oauth_credential,
        },
        notification::slack::{mock_slack_auth_test, slack_auth_test_response},
    },
    state::{Remembered, StoredTokens},
    world::ApiWorld,
};

const MY_TOKEN: &str = "slack_test_user_access_token";
const OTHERS_TOKEN: &str = "other_user_slack_access_token";
const REVOKED_TOKEN: &str = "slack_revoked_access_token";

enum Mode {
    Apply,
    DryRun,
}

fn credential_with_token(token: &str) -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken(token.to_string()),
        ..slack_oauth_credential()
    }
}

fn the_other_user(world: &ApiWorld) -> UserId {
    world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id
}

async fn connect(
    world: &mut ApiWorld,
    user_id: UserId,
    credential: OAuthCredentialFixture,
    context: Option<IntegrationConnectionContext>,
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
    let user_id = world.user().id;
    let credential = OAuthCredentialFixture {
        access_token_expires_at: Some(Utc::now() - TimeDelta::hours(1)),
        ..slack_oauth_credential()
    };
    let id = connect(world, user_id, credential, None).await;
    world.slack_backfill.mine = Some(id);
}

#[given("another user has a Slack connection with a revoked access token, and no team yet")]
async fn another_user_has_a_connection_with_a_revoked_token(world: &mut ApiWorld) {
    let user_id = the_other_user(world);
    let id = connect(world, user_id, credential_with_token(REVOKED_TOKEN), None).await;
    world.slack_backfill.others = Some(id);
}

#[given("another user has a Slack connection with an access token of their own, and no team yet")]
async fn another_user_has_a_connection_with_their_own_token(world: &mut ApiWorld) {
    let user_id = the_other_user(world);
    let id = connect(world, user_id, credential_with_token(OTHERS_TOKEN), None).await;
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

#[given(
    expr = "Slack tells that the access token of that other user belongs to the team {string}, and is asked once"
)]
async fn slack_knows_their_token(world: &mut ApiWorld, team: String) {
    let app = world.app.get().await;
    mock_slack_auth_test(
        &app.slack_mock_server,
        OTHERS_TOKEN,
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

async fn remembered_state(world: &mut ApiWorld, id: IntegrationConnectionId) -> Remembered {
    let connection = connection(world, id).await;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let credential = app
        .repository
        .get_oauth_credential(&mut transaction, id)
        .await
        .unwrap()
        .map(|stored| StoredTokens {
            encrypted_access_token: stored.encrypted_access_token,
            encrypted_refresh_token: stored.encrypted_refresh_token,
            access_token_expires_at: stored.access_token_expires_at,
            updated_at: stored.updated_at,
        });
    transaction.commit().await.unwrap();
    Remembered {
        connection,
        credential,
    }
}

fn my_connection(world: &ApiWorld) -> IntegrationConnectionId {
    world
        .slack_backfill
        .mine
        .expect("I have no Slack connection")
}

fn their_connection(world: &ApiWorld) -> IntegrationConnectionId {
    world
        .slack_backfill
        .others
        .expect("The other user has no Slack connection")
}

#[given("I remember my Slack connection and its credential")]
async fn i_remember_my_connection(world: &mut ApiWorld) {
    let id = my_connection(world);
    world.slack_backfill.remembered = Some(remembered_state(world, id).await);
}

#[then("my Slack connection and its credential are exactly as I remembered them")]
async fn my_connection_is_as_remembered(world: &mut ApiWorld) {
    let id = my_connection(world);
    let now = remembered_state(world, id).await;
    assert_eq!(Some(now), world.slack_backfill.remembered.take());
}

#[then(expr = "my Slack connection is in the team {string}")]
async fn my_connection_is_in_the_team(world: &mut ApiWorld, team: String) {
    let id = my_connection(world);
    assert_eq!(
        connection(world, id).await.provider.context(),
        Some(slack_context(&team))
    );
}

#[then(expr = "the Slack connection of that other user is in the team {string}")]
async fn the_other_connection_is_in_the_team(world: &mut ApiWorld, team: String) {
    let id = their_connection(world);
    assert_eq!(
        connection(world, id).await.provider.context(),
        Some(slack_context(&team))
    );
}

#[then("my Slack connection has no team")]
async fn my_connection_has_no_team(world: &mut ApiWorld) {
    let id = my_connection(world);
    assert_eq!(connection(world, id).await.provider.context(), None);
}

#[then("the Slack connection of that other user has no team")]
async fn the_other_connection_has_no_team(world: &mut ApiWorld) {
    let id = their_connection(world);
    assert_eq!(connection(world, id).await.provider.context(), None);
}

#[then("my Slack connection is still validated")]
async fn my_connection_is_still_validated(world: &mut ApiWorld) {
    let id = my_connection(world);
    assert_eq!(
        connection(world, id).await.status,
        IntegrationConnectionStatus::Validated
    );
}

#[then("the Slack connection of that other user is still validated")]
async fn the_other_connection_is_still_validated(world: &mut ApiWorld) {
    let id = their_connection(world);
    assert_eq!(
        connection(world, id).await.status,
        IntegrationConnectionStatus::Validated
    );
}
