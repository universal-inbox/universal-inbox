use std::{str::FromStr, sync::Arc};

use chrono::{DateTime, Utc};
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use uuid::Uuid;

use universal_inbox::{
    billing::{Plan, SubscriptionStatus, UserSubscription},
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig, google_calendar::GoogleCalendarConfig,
            google_drive::GoogleDriveConfig, google_mail::GoogleMailConfig,
        },
        provider::IntegrationProviderKind,
    },
};
use universal_inbox_api::{
    billing::{
        repository::BillingRepository,
        service::{BillingService, RepositoryIntegrationCounter},
        stripe::{
            CheckoutSessionParams, PortalSessionParams, RawSubscription, StripeClient, StripeError,
            StripeEvent, StripeEventKind,
        },
    },
    configuration::FreePlanSettings,
    repository::integration_connection::IntegrationConnectionRepository,
    universal_inbox::UniversalInboxError,
};

use crate::{
    helpers::{
        TestedApp, integration_connection::create_integration_connection, user::create_user,
    },
    world::ApiWorld,
};

const BASE: i64 = 1_700_000_000;

#[derive(Clone, Copy, Debug)]
pub struct StripeStatus(SubscriptionStatus);

impl Parameter for StripeStatus {
    const REGEX: &'static str = "active|past due|canceled|incomplete";
    const NAME: &'static str = "stripe_status";
}

impl FromStr for StripeStatus {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(Self(match text {
            "active" => SubscriptionStatus::Active,
            "past due" => SubscriptionStatus::PastDue,
            "canceled" => SubscriptionStatus::Canceled,
            "incomplete" => SubscriptionStatus::Incomplete,
            other => return Err(format!("Unknown subscription status {other:?}")),
        }))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlanName(Plan);

impl Parameter for PlanName {
    const REGEX: &'static str = "Free|Paid";
    const NAME: &'static str = "plan";
}

impl FromStr for PlanName {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "Free" => Ok(Self(Plan::Free)),
            "Paid" => Ok(Self(Plan::Paid)),
            other => Err(format!("Unknown plan {other:?}")),
        }
    }
}

fn at(second: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(BASE + second, 0).expect("valid timestamp")
}

fn raw_sub(subscription: &str, customer: &str, status: SubscriptionStatus) -> RawSubscription {
    RawSubscription {
        subscription_id: subscription.to_string(),
        customer_id: customer.to_string(),
        price_id: Some("price_test".to_string()),
        status,
        current_period_start: None,
        current_period_end: None,
        cancel_at_period_end: false,
        canceled_at: None,
    }
}

struct StubStripeClient {
    customer_subscriptions: Vec<RawSubscription>,
}

#[async_trait::async_trait]
impl StripeClient for StubStripeClient {
    async fn create_customer(
        &self,
        _user_id: universal_inbox::user::UserId,
        _email: Option<&str>,
    ) -> Result<String, StripeError> {
        Ok("cus_test_stub".to_string())
    }

    async fn create_checkout_session(
        &self,
        _params: CheckoutSessionParams,
    ) -> Result<url::Url, StripeError> {
        Ok(url::Url::parse("https://checkout.stripe.test/session-stub").unwrap())
    }

    async fn create_portal_session(
        &self,
        _params: PortalSessionParams,
    ) -> Result<url::Url, StripeError> {
        Ok(url::Url::parse("https://portal.stripe.test/session-stub").unwrap())
    }

    async fn fetch_subscription(
        &self,
        _subscription_id: &str,
    ) -> Result<RawSubscription, StripeError> {
        Err(StripeError::Other(anyhow::anyhow!(
            "fetch_subscription is not exercised by these scenarios"
        )))
    }

    async fn list_customer_subscriptions(
        &self,
        _customer_id: &str,
    ) -> Result<Vec<RawSubscription>, StripeError> {
        Ok(self.customer_subscriptions.clone())
    }

    async fn cancel_subscription(
        &self,
        _subscription_id: &str,
    ) -> Result<RawSubscription, StripeError> {
        Err(StripeError::Other(anyhow::anyhow!(
            "cancel_subscription is not exercised by these scenarios"
        )))
    }

    async fn clear_customer_user_id(&self, _customer_id: &str) -> Result<(), StripeError> {
        Ok(())
    }

    fn verify_webhook_signature(
        &self,
        _payload: &str,
        _signature_header: &str,
    ) -> Result<StripeEvent, StripeError> {
        Err(StripeError::InvalidWebhookSignature)
    }
}

fn service(app: &TestedApp, customer_subscriptions: Vec<RawSubscription>) -> Arc<BillingService> {
    Arc::new(BillingService::new(
        app.repository.clone(),
        Arc::new(StubStripeClient {
            customer_subscriptions,
        }),
        &FreePlanSettings::default(),
        universal_inbox::billing::StripePriceId("price_test".to_string()),
        Arc::new(RepositoryIntegrationCounter {
            repository: app.repository.clone(),
        }),
    ))
}

async fn billing(world: &mut ApiWorld) -> (Arc<BillingService>, &TestedApp) {
    let holds = world.billing.stripe_holds.clone();
    let app = world.app.get().await;
    (service(app, holds), app)
}

fn user_id(world: &ApiWorld) -> universal_inbox::user::UserId {
    world.billing.user_id.expect("No billing user exists")
}

async fn limit(world: &mut ApiWorld) -> usize {
    let (billing, _) = billing(world).await;
    billing.limits().max_integration_connections as usize
}

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

async fn read_subscription(world: &mut ApiWorld) -> Option<UserSubscription> {
    let user_id = user_id(world);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let subscription = app
        .repository
        .get_user_subscription(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    subscription
}

async fn connections(world: &mut ApiWorld) -> Vec<IntegrationConnection> {
    let user_id = user_id(world);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connections = app
        .repository
        .fetch_all_integration_connections(&mut transaction, user_id, None, false)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    connections
}

async fn apply(world: &mut ApiWorld, event: StripeEvent, prefetched: Option<RawSubscription>) {
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    billing
        .apply_subscription_event(&mut transaction, &event, prefetched)
        .await
        .expect("apply_subscription_event should succeed");
    transaction.commit().await.unwrap();
}

fn event(second: i64, label: &str, kind: StripeEventKind) -> StripeEvent {
    StripeEvent {
        id: format!("evt_{label}_{second}"),
        type_: label.to_string(),
        created: at(second),
        kind,
    }
}

#[given("a user with a password exists")]
async fn a_user_exists(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let user = create_user(
        app,
        format!("{}@billing.test", Uuid::new_v4()).parse().unwrap(),
        "correct horse battery staple",
    )
    .await;
    world.billing.user_id = Some(user.id);
}

#[given(expr = "Stripe holds the subscription {string} of the customer {string}, {stripe_status}")]
fn stripe_holds_a_subscription(
    world: &mut ApiWorld,
    subscription: String,
    customer: String,
    status: StripeStatus,
) {
    world
        .billing
        .stripe_holds
        .push(raw_sub(&subscription, &customer, status.0));
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

#[when(
    expr = "Stripe reports at second {int} that the customer {string} checked out with the {stripe_status} subscription {string}"
)]
#[given(
    expr = "Stripe reports at second {int} that the customer {string} checked out with the {stripe_status} subscription {string}"
)]
async fn stripe_reports_a_checkout(
    world: &mut ApiWorld,
    second: i64,
    customer: String,
    status: StripeStatus,
    subscription: String,
) {
    let user_id = user_id(world);
    let event = event(
        second,
        "checkout.session.completed",
        StripeEventKind::CheckoutSessionCompleted {
            customer_id: Some(customer.clone()),
            subscription_id: Some(subscription.clone()),
            user_id_metadata: Some(user_id),
        },
    );
    let event = StripeEvent {
        id: format!("evt_checkout_{}", at(second).timestamp()),
        ..event
    };
    apply(
        world,
        event,
        Some(raw_sub(&subscription, &customer, status.0)),
    )
    .await;
}

#[when(
    expr = "Stripe reports at second {int} that the customer {string} checked out, without any subscription yet"
)]
#[given(
    expr = "Stripe reports at second {int} that the customer {string} checked out, without any subscription yet"
)]
async fn stripe_reports_a_checkout_without_subscription(
    world: &mut ApiWorld,
    second: i64,
    customer: String,
) {
    let user_id = user_id(world);
    let event = event(
        second,
        "checkout_no_subscription",
        StripeEventKind::CheckoutSessionCompleted {
            customer_id: Some(customer),
            subscription_id: None,
            user_id_metadata: Some(user_id),
        },
    );
    apply(world, event, None).await;
}

#[when(
    expr = "Stripe reports at second {int} that the subscription {string} of the customer {string} was updated to {stripe_status}"
)]
#[given(
    expr = "Stripe reports at second {int} that the subscription {string} of the customer {string} was updated to {stripe_status}"
)]
async fn stripe_reports_an_update(
    world: &mut ApiWorld,
    second: i64,
    subscription: String,
    customer: String,
    status: StripeStatus,
) {
    let event = event(
        second,
        "customer.subscription.updated",
        StripeEventKind::SubscriptionUpserted(raw_sub(&subscription, &customer, status.0)),
    );
    apply(world, event, None).await;
}

#[when(
    expr = "Stripe reports at second {int} that the subscription {string} of the customer {string} was deleted"
)]
#[given(
    expr = "Stripe reports at second {int} that the subscription {string} of the customer {string} was deleted"
)]
async fn stripe_reports_a_deletion(
    world: &mut ApiWorld,
    second: i64,
    subscription: String,
    customer: String,
) {
    let event = event(
        second,
        "customer.subscription.deleted",
        StripeEventKind::SubscriptionDeleted(raw_sub(
            &subscription,
            &customer,
            SubscriptionStatus::Canceled,
        )),
    );
    apply(world, event, None).await;
}

#[when(
    expr = "Stripe reports at second {int} that the invoice of the subscription {string} of the customer {string} was paid, the subscription being {stripe_status}"
)]
async fn stripe_reports_a_paid_invoice(
    world: &mut ApiWorld,
    second: i64,
    subscription: String,
    customer: String,
    status: StripeStatus,
) {
    let event = event(
        second,
        "invoice.paid",
        StripeEventKind::InvoicePaid {
            customer_id: Some(customer.clone()),
            subscription_id: Some(subscription.clone()),
        },
    );
    apply(
        world,
        event,
        Some(raw_sub(&subscription, &customer, status.0)),
    )
    .await;
}

#[when(
    expr = "a write from second {int} that says {stripe_status} and {plan} is attempted on the subscription"
)]
async fn a_write_is_attempted(
    world: &mut ApiWorld,
    second: i64,
    status: StripeStatus,
    plan: PlanName,
) {
    let current = read_subscription(world).await.expect("No subscription row");
    let write = UserSubscription {
        status: status.0,
        plan: plan.0,
        canceled_at: None,
        last_stripe_event_at: Some(at(second)),
        ..current
    };
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let returned = app
        .repository
        .upsert_user_subscription(&mut transaction, &write)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.billing.written_status = Some(returned.status);
}

#[when("the reconcile job runs")]
async fn the_reconcile_job_runs(world: &mut ApiWorld) {
    let (billing, _) = billing(world).await;
    world.billing.report = Some(billing.reconcile_subscriptions(false).await.unwrap());
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

#[then("the user has no subscription")]
async fn the_user_has_no_subscription(world: &mut ApiWorld) {
    assert!(read_subscription(world).await.is_none());
}

#[then(expr = "the subscription is linked to the customer {string}, without any subscription id")]
async fn the_subscription_has_only_a_customer(world: &mut ApiWorld, customer: String) {
    let subscription = read_subscription(world).await.expect("No subscription row");
    assert_eq!(
        subscription.stripe_customer_id.as_ref().map(|c| c.as_str()),
        Some(customer.as_str())
    );
    assert!(subscription.stripe_subscription_id.is_none());
}

#[then(expr = "the subscription is linked to the customer {string} and the subscription {string}")]
async fn the_subscription_is_linked(world: &mut ApiWorld, customer: String, subscription: String) {
    let row = read_subscription(world).await.expect("No subscription row");
    assert_eq!(
        row.stripe_customer_id.as_ref().map(|c| c.as_str()),
        Some(customer.as_str())
    );
    assert_eq!(
        row.stripe_subscription_id.as_ref().map(|s| s.as_str()),
        Some(subscription.as_str())
    );
}

#[then(expr = "the subscription of the user is {stripe_status}")]
async fn the_subscription_is(world: &mut ApiWorld, status: StripeStatus) {
    assert_eq!(read_subscription(world).await.unwrap().status, status.0);
}

#[then(expr = "the user is on the {plan} plan")]
async fn the_user_is_on_the_plan(world: &mut ApiWorld, plan: PlanName) {
    assert_eq!(
        read_subscription(world).await.unwrap().effective_plan(),
        plan.0
    );
}

#[then(expr = "the write reports the subscription as {stripe_status}")]
fn the_write_reports(world: &mut ApiWorld, status: StripeStatus) {
    assert_eq!(world.billing.written_status, Some(status.0));
}

#[then("the watermark of the ordering guard did not move back")]
async fn the_watermark_did_not_move_back(world: &mut ApiWorld) {
    let subscription = read_subscription(world).await.unwrap();
    assert_eq!(subscription.last_stripe_event_at, Some(at(200)));
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

#[then(expr = "the reconcile job adopted {int} and refreshed {int} subscription(s)")]
fn the_report_adopted(world: &mut ApiWorld, adopted: usize, refreshed: usize) {
    let report = world
        .billing
        .report
        .as_ref()
        .expect("The reconcile job did not run");
    assert_eq!((report.adopted, report.refreshed), (adopted, refreshed));
}

#[then(expr = "the reconcile job adopted {int} subscription(s)")]
fn the_report_adopted_only(world: &mut ApiWorld, adopted: usize) {
    assert_eq!(world.billing.report.as_ref().unwrap().adopted, adopted);
}

#[then(expr = "the reconcile job enforced {int} expired grace deadline(s)")]
fn the_report_enforced(world: &mut ApiWorld, count: usize) {
    assert_eq!(world.billing.report.as_ref().unwrap().grace_expired, count);
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

#[derive(Clone, Copy, Debug)]
pub struct SyncOf(universal_inbox_api::billing::service::SyncKind);

impl Parameter for SyncOf {
    const REGEX: &'static str = "notifications|tasks";
    const NAME: &'static str = "synced_items";
}

impl FromStr for SyncOf {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        use universal_inbox_api::billing::service::SyncKind;
        match text {
            "notifications" => Ok(Self(SyncKind::Notifications)),
            "tasks" => Ok(Self(SyncKind::Tasks)),
            other => Err(format!("Unknown sync kind {other:?}")),
        }
    }
}

#[then(
    expr = "the {plan} plan syncs {synced_items} every {int} minutes, when the global minimum is {int} minutes"
)]
async fn the_plan_syncs_every(
    world: &mut ApiWorld,
    plan: PlanName,
    kind: SyncOf,
    minutes: i64,
    global: i64,
) {
    let (billing, _) = billing(world).await;
    assert_eq!(
        universal_inbox_api::billing::service::effective_sync_interval(
            billing.limits(),
            plan.0,
            global,
            kind.0
        ),
        minutes
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

#[given("Stripe events were received 40 days ago and just now")]
async fn stripe_events_of_two_ages(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    for event_id in ["evt_old", "evt_new"] {
        assert!(
            app.repository
                .record_stripe_event(&mut transaction, event_id, "test")
                .await
                .unwrap()
        );
    }
    sqlx::query("UPDATE stripe_event SET received_at = now() - interval '40 days' WHERE event_id = 'evt_old'")
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn prune(world: &mut ApiWorld, dry_run: bool) -> u64 {
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    let pruned = billing
        .prune_stripe_events(&mut transaction, dry_run)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    pruned
}

#[then("a dry run of the pruning removes nothing")]
async fn a_dry_run_removes_nothing(world: &mut ApiWorld) {
    assert_eq!(prune(world, true).await, 0);
}

#[then("the pruning removes exactly the event of 40 days ago")]
async fn the_pruning_removes_the_old_event(world: &mut ApiWorld) {
    assert_eq!(prune(world, false).await, 1);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let old_is_gone = app
        .repository
        .record_stripe_event(&mut transaction, "evt_old", "test")
        .await
        .unwrap();
    let new_is_kept = !app
        .repository
        .record_stripe_event(&mut transaction, "evt_new", "test")
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert!(old_is_gone, "the old event survived");
    assert!(new_is_kept, "the fresh event was pruned");
}
