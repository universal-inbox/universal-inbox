use chrono::Utc;
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;

use universal_inbox::integration_connection::{
    IntegrationConnection, IntegrationConnectionStatus,
    config::IntegrationConnectionConfig,
    integrations::{
        github::GithubConfig, google_calendar::GoogleCalendarConfig,
        google_drive::GoogleDriveConfig, google_mail::GoogleMailConfig,
    },
    provider::IntegrationProviderKind,
};
use universal_inbox_api::{
    billing::repository::BillingRepository,
    repository::integration_connection::IntegrationConnectionRepository,
    universal_inbox::UniversalInboxError,
};

use crate::{helpers::integration_connection::create_integration_connection, world::ApiWorld};

use crate::steps::billing_support::*;

fn notification_sync_configs() -> Vec<IntegrationConnectionConfig> {
    vec![
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled()),
    ]
}

async fn seed_validated(
    world: &mut ApiWorld,
    config: IntegrationConnectionConfig,
) -> IntegrationConnection {
    let user_id = user_id(world);
    let app = world.app.get().await;
    *create_integration_connection(
        app,
        user_id,
        config,
        IntegrationConnectionStatus::Validated,
        None,
        None,
        None,
        None,
        None,
    )
    .await
}

#[given("the user validates one more integration than the Free plan allows")]
async fn the_user_exceeds_the_free_plan(world: &mut ApiWorld) {
    let limit = limit(world).await;
    let configs = notification_sync_configs();
    assert!(
        configs.len() > limit,
        "needs at least limit+1 distinct provider configs"
    );
    for config in configs.into_iter().take(limit + 1) {
        let connection = seed_validated(world, config).await;
        world.billing.seeded.push(connection.id);
    }
}

#[given("the user validates as many integrations as the Free plan allows")]
async fn the_user_is_at_the_cap(world: &mut ApiWorld) {
    let limit = limit(world).await;
    for config in notification_sync_configs().into_iter().take(limit) {
        let connection = seed_validated(world, config).await;
        world.billing.seeded.push(connection.id);
    }
}

#[given("the user validates every integration that syncs notifications")]
async fn the_user_validates_all(world: &mut ApiWorld) {
    for config in notification_sync_configs() {
        seed_validated(world, config).await;
    }
}

#[given("the user validates a Google Calendar integration, whose sync a pause cannot switch off")]
async fn the_user_validates_a_calendar(world: &mut ApiWorld) {
    let connection = seed_validated(
        world,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
    )
    .await;
    world.billing.calendar = Some(connection.id);
}

#[given("the user validates a Todoist integration and the implicit API connection")]
async fn the_user_validates_todoist_and_api(world: &mut ApiWorld) {
    seed_validated(
        world,
        IntegrationConnectionConfig::Todoist(Default::default()),
    )
    .await;
    seed_validated(world, IntegrationConnectionConfig::API).await;
}

#[given("an older enforcement marked that Google Calendar connection as paused")]
async fn an_older_enforcement_paused_it(world: &mut ApiWorld) {
    let id = world.billing.calendar.expect("No calendar connection");
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .set_integration_connection_plan_pause(
            &mut transaction,
            id,
            Some(Utc::now()),
            Some(&IntegrationConnectionConfig::GoogleCalendar(
                GoogleCalendarConfig::enabled(),
            )),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[when("the over-limit grace deadline of the user is armed")]
async fn the_grace_deadline_is_armed(world: &mut ApiWorld) {
    let user_id = user_id(world);
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    let deadline = billing
        .mark_over_limit_grace_deadline_if_needed(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.billing.armed.push(deadline);
}

#[when(expr = "the grace deadline of the user is set to second {int}")]
async fn the_grace_deadline_is_set(world: &mut ApiWorld, second: i64) {
    let user_id = user_id(world);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .set_over_limit_grace_deadline(&mut transaction, user_id, Some(at(second)))
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[when("the first integration of the user is no longer validated")]
async fn the_first_integration_is_no_longer_validated(world: &mut ApiWorld) {
    let user_id = user_id(world);
    let id = world.billing.seeded[0];
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_integration_connection_status(
            &mut transaction,
            id,
            IntegrationConnectionStatus::Created,
            None,
            None,
            user_id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[when("the Free plan is enforced on the user")]
async fn the_free_plan_is_enforced(world: &mut ApiWorld) {
    let user_id = user_id(world);
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    let paused = billing
        .enforce_free_plan_compliance(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.billing.paused.push(paused);
}

#[when("the usage of the user is counted, and the grace deadline is armed again")]
async fn the_usage_is_counted(world: &mut ApiWorld) {
    let user_id = user_id(world);
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    let used = app
        .repository
        .count_validated_integration_connections(&mut transaction, user_id)
        .await
        .unwrap();
    let armed = billing
        .mark_over_limit_grace_deadline_if_needed(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.billing.used = Some(used);
    world.billing.armed.push(armed);
}

#[when("the validated integrations of the user are counted")]
async fn the_validated_integrations_are_counted(world: &mut ApiWorld) {
    let user_id = user_id(world);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let used = app
        .repository
        .count_validated_integration_connections(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.billing.used = Some(used);
}

async fn ask_the_cap(world: &mut ApiWorld, kind: IntegrationProviderKind) {
    let user_id = user_id(world);
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    let verdict = billing
        .assert_can_add_integration(&mut transaction, user_id, kind)
        .await;
    transaction.commit().await.unwrap();
    world.billing.verdict = Some(match verdict {
        Ok(()) => Ok(()),
        Err(UniversalInboxError::PaymentRequired { .. }) => Err("payment required".to_string()),
        Err(other) => Err(format!("{other:?}")),
    });
}

#[when("the user adds the implicit API connection")]
async fn the_user_adds_the_api_connection(world: &mut ApiWorld) {
    ask_the_cap(world, IntegrationProviderKind::API).await;
}

#[when("the user adds a GitHub integration")]
async fn the_user_adds_a_github_integration(world: &mut ApiWorld) {
    ask_the_cap(world, IntegrationProviderKind::Github).await;
}

#[then("the over-limit grace deadline of the user is armed")]
async fn the_grace_deadline_is_armed_on_the_row(world: &mut ApiWorld) {
    assert!(
        read_subscription(world)
            .await
            .unwrap()
            .over_limit_grace_deadline
            .is_some()
    );
}

#[then("no over-limit grace deadline of the user is armed")]
async fn no_grace_deadline_is_armed(world: &mut ApiWorld) {
    assert!(
        read_subscription(world)
            .await
            .unwrap()
            .over_limit_grace_deadline
            .is_none()
    );
}

#[then("arming it a second time gives the same deadline")]
fn arming_is_idempotent(world: &mut ApiWorld) {
    let armed = &world.billing.armed;
    assert!(armed[0].is_some());
    assert_eq!(armed[1], armed[0]);
}

#[then("the last arming gave no deadline")]
fn the_last_arming_gave_none(world: &mut ApiWorld) {
    assert_eq!(world.billing.armed.last(), Some(&None));
}

#[then("every integration of the user is still enabled and not paused")]
async fn every_integration_is_enabled(world: &mut ApiWorld) {
    for connection in connections(world).await {
        assert!(connection.provider.is_sync_notifications_enabled());
        assert!(connection.auto_paused_by_plan_at.is_none());
    }
}

#[then(
    expr = "{int} integration(s) of the user is/are paused, with the sync switched off and the previous config kept"
)]
async fn integrations_are_paused(world: &mut ApiWorld, count: usize) {
    let connections = connections(world).await;
    let paused: Vec<_> = connections
        .iter()
        .filter(|connection| connection.auto_paused_by_plan_at.is_some())
        .collect();
    assert_eq!(paused.len(), count);
    for connection in paused {
        assert!(!connection.provider.is_sync_notifications_enabled());
        assert!(connection.auto_paused_config_snapshot.is_some());
    }
}

#[then(expr = "{int} integration(s) of the user still sync(s) notifications")]
async fn integrations_still_sync(world: &mut ApiWorld, count: usize) {
    let synced = connections(world)
        .await
        .iter()
        .filter(|connection| connection.provider.is_sync_notifications_enabled())
        .count();
    assert_eq!(synced, count);
}

#[then("the usage of the user is the number of integrations that the Free plan allows")]
async fn the_usage_is_the_limit(world: &mut ApiWorld) {
    let limit = limit(world).await;
    assert_eq!(world.billing.used, Some(limit as u32));
}

#[then(expr = "the usage of the user is {int}")]
fn the_usage_is(world: &mut ApiWorld, used: u32) {
    assert_eq!(world.billing.used, Some(used));
}

#[then(expr = "the enforcement paused {int} integration(s)")]
fn the_enforcement_paused(world: &mut ApiWorld, count: usize) {
    assert_eq!(world.billing.paused.last().map(Vec::len), Some(count));
}

#[then("the paused integration is one whose sync a pause can switch off")]
fn the_paused_is_togglable(world: &mut ApiWorld) {
    let paused = world.billing.paused.last().unwrap();
    assert!(world.billing.seeded.contains(&paused[0]));
    assert!(!paused.contains(&world.billing.calendar.unwrap()));
}

#[then("the Google Calendar connection is not marked as paused")]
async fn the_calendar_is_not_paused(world: &mut ApiWorld) {
    let id = world.billing.calendar.unwrap();
    let connections = connections(world).await;
    let calendar = connections.iter().find(|c| c.id == id).unwrap();
    assert!(calendar.auto_paused_by_plan_at.is_none());
}

#[then("the paused integration keeps the config that the user had, not the disabled one")]
async fn the_snapshot_is_the_original(world: &mut ApiWorld) {
    let paused_id = world.billing.paused[0][0];
    let connection = connections(world)
        .await
        .into_iter()
        .find(|c| c.id == paused_id)
        .unwrap();
    let snapshot = connection
        .auto_paused_config_snapshot
        .clone()
        .expect("No snapshot");
    assert_ne!(snapshot, connection.provider.config());
    world.billing.snapshot = Some(snapshot);
}

#[then("the paused integration still keeps the same config")]
async fn the_snapshot_is_unchanged(world: &mut ApiWorld) {
    let paused_id = world.billing.paused[0][0];
    let snapshot = connections(world)
        .await
        .into_iter()
        .find(|c| c.id == paused_id)
        .and_then(|c| c.auto_paused_config_snapshot)
        .expect("No snapshot");
    assert_eq!(Some(snapshot), world.billing.snapshot);
}

#[then(
    "every integration of the user is restored to its enabled config, without a pause marker or a snapshot"
)]
async fn every_integration_is_restored(world: &mut ApiWorld) {
    for connection in connections(world).await {
        assert!(connection.auto_paused_by_plan_at.is_none());
        assert!(connection.auto_paused_config_snapshot.is_none());
        assert!(connection.provider.is_sync_notifications_enabled());
    }
}

#[then("the cap allows it")]
fn the_cap_allows_it(world: &mut ApiWorld) {
    assert_eq!(world.billing.verdict, Some(Ok(())));
}

#[then("the cap refuses it, asking for a payment")]
fn the_cap_refuses_it(world: &mut ApiWorld) {
    assert_eq!(
        world.billing.verdict,
        Some(Err("payment required".to_string()))
    );
}

#[then(
    "the integrations that the Free plan allows still sync notifications, and the others do not"
)]
async fn the_allowance_still_syncs(world: &mut ApiWorld) {
    let limit = limit(world).await;
    let synced = connections(world)
        .await
        .iter()
        .filter(|connection| connection.provider.is_sync_notifications_enabled())
        .count();
    assert_eq!(synced, limit);
}

#[then("that count is the number of validated integrations that load one by one")]
async fn the_count_matches_the_rows(world: &mut ApiWorld) {
    let user_id = user_id(world);
    let used = world.billing.used.expect("Nothing was counted");
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let loaded = app
        .repository
        .fetch_all_integration_connections(
            &mut transaction,
            user_id,
            Some(IntegrationConnectionStatus::Validated),
            false,
        )
        .await
        .unwrap()
        .len() as u32;
    transaction.commit().await.unwrap();
    assert_eq!(used, loaded);
}

#[given("another user with a password validates every integration that syncs notifications")]
async fn the_other_user_validates_all(world: &mut ApiWorld) {
    let other = create_the_other_user(world).await;
    let app = world.app.get().await;
    for config in notification_sync_configs() {
        create_integration_connection(
            app,
            other,
            config,
            IntegrationConnectionStatus::Validated,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    }
}

#[when("the over-limit grace deadline of the other user is armed")]
async fn the_grace_deadline_of_the_other_user_is_armed(world: &mut ApiWorld) {
    let other = the_other_user_id(world);
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    let deadline = billing
        .mark_over_limit_grace_deadline_if_needed(&mut transaction, other)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert!(deadline.is_some(), "the other user is over the limit");
}

#[then("none of the integrations of the other user is paused, and all of them are still validated")]
async fn the_other_user_is_untouched(world: &mut ApiWorld) {
    let other = the_other_user_id(world);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connections = app
        .repository
        .fetch_all_integration_connections(&mut transaction, other, None, false)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(connections.len(), notification_sync_configs().len());
    for connection in connections {
        assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
        assert!(connection.auto_paused_by_plan_at.is_none());
        assert!(connection.provider.is_sync_notifications_enabled());
    }
}

#[then(expr = "the usage of the other user is {int}")]
async fn the_usage_of_the_other_user_is(world: &mut ApiWorld, used: u32) {
    let other = the_other_user_id(world);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let counted = app
        .repository
        .count_validated_integration_connections(&mut transaction, other)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(counted, used);
}

#[when("the user adds a Todoist integration")]
async fn the_user_adds_a_todoist_integration(world: &mut ApiWorld) {
    ask_the_cap(world, IntegrationProviderKind::Todoist).await;
}

#[given("the user validates one integration fewer than the Free plan allows")]
async fn the_user_is_one_below_the_cap(world: &mut ApiWorld) {
    let limit = limit(world).await;
    for config in notification_sync_configs().into_iter().take(limit - 1) {
        let connection = seed_validated(world, config).await;
        world.billing.seeded.push(connection.id);
    }
}
