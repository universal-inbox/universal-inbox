use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;

use universal_inbox::billing::{SubscriptionStatus, UserSubscription};
use universal_inbox_api::billing::{
    repository::BillingRepository,
    stripe::{StripeEvent, StripeEventKind},
};

use crate::world::ApiWorld;

use crate::steps::billing_support::*;

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

#[then(expr = "the write reports the subscription as {stripe_status}")]
fn the_write_reports(world: &mut ApiWorld, status: StripeStatus) {
    assert_eq!(world.billing.written_status, Some(status.0));
}

#[then("the watermark of the ordering guard did not move back")]
async fn the_watermark_did_not_move_back(world: &mut ApiWorld) {
    let subscription = read_subscription(world).await.unwrap();
    assert_eq!(subscription.last_stripe_event_at, Some(at(200)));
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
    transaction.commit().await.unwrap();
    crate::helpers::billing::backdate_stripe_event(app, "evt_old", 40).await;
}

async fn prune(world: &mut ApiWorld, dry_run: bool) {
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    let pruned = billing
        .prune_stripe_events(&mut transaction, dry_run)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.billing.pruned = Some(pruned);
}

#[when("the Stripe events are pruned, as a dry run")]
async fn the_events_are_pruned_as_a_dry_run(world: &mut ApiWorld) {
    prune(world, true).await;
}

#[when("the Stripe events are pruned")]
async fn the_events_are_pruned(world: &mut ApiWorld) {
    prune(world, false).await;
}

#[then(expr = "the pruning removed {int} event(s)")]
fn the_pruning_removed(world: &mut ApiWorld, count: u64) {
    assert_eq!(world.billing.pruned, Some(count));
}

#[then("only the event of 40 days ago is gone")]
async fn only_the_old_event_is_gone(world: &mut ApiWorld) {
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

#[when(
    expr = "I look up the sync interval of the {plan} plan for {synced_items}, with a global minimum of {int} minutes"
)]
async fn i_look_up_the_interval(world: &mut ApiWorld, plan: PlanName, kind: SyncOf, global: i64) {
    let (billing, _) = billing(world).await;
    world.billing.interval = Some(
        universal_inbox_api::billing::service::effective_sync_interval(
            billing.limits(),
            plan.0,
            global,
            kind.0,
        ),
    );
}

#[then(expr = "that interval is {int} minutes")]
fn that_interval_is(world: &mut ApiWorld, minutes: i64) {
    assert_eq!(world.billing.interval, Some(minutes));
}

#[given(
    expr = "another user with a password pays, as the customer {string} with the {stripe_status} subscription {string}"
)]
async fn another_user_pays(
    world: &mut ApiWorld,
    customer: String,
    status: StripeStatus,
    subscription: String,
) {
    let other = create_the_other_user(world).await;
    let event = event(
        0,
        "checkout.session.completed",
        StripeEventKind::CheckoutSessionCompleted {
            customer_id: Some(customer.clone()),
            subscription_id: Some(subscription.clone()),
            user_id_metadata: Some(other),
        },
    );
    apply(
        world,
        event,
        Some(raw_sub(&subscription, &customer, status.0)),
    )
    .await;
}

#[then(expr = "the subscription of the other user is {stripe_status}")]
async fn the_subscription_of_the_other_user_is(world: &mut ApiWorld, status: StripeStatus) {
    let other = the_other_user_id(world);
    assert_eq!(
        read_subscription_of(world, other).await.unwrap().status,
        status.0
    );
}

#[then(expr = "the other user is on the {plan} plan")]
async fn the_other_user_is_on_the_plan(world: &mut ApiWorld, plan: PlanName) {
    let other = the_other_user_id(world);
    assert_eq!(
        read_subscription_of(world, other)
            .await
            .unwrap()
            .effective_plan(),
        plan.0
    );
}
