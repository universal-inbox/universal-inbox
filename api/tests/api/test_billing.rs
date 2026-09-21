use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use rstest::*;
use serde_json::json;
use universal_inbox::billing::{Plan, SubscriptionStatus as DomainStatus};
use universal_inbox::user::UserId;
use universal_inbox_api::billing::repository::BillingRepository;
use universal_inbox_api::billing::service::{BillingService, ReconcileReport};
use universal_inbox_api::billing::stripe::{RawSubscription, StripeEvent, StripeEventKind};

use universal_inbox::integration_connection::{
    IntegrationConnection, IntegrationConnectionStatus,
    config::IntegrationConnectionConfig,
    integrations::{
        github::GithubConfig, google_calendar::GoogleCalendarConfig,
        google_drive::GoogleDriveConfig, google_mail::GoogleMailConfig,
    },
};
use universal_inbox_api::repository::integration_connection::IntegrationConnectionRepository;

use crate::helpers::{TestedApp, tested_app, tested_app_with_local_auth};

fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, 0).expect("valid timestamp")
}

fn raw_sub(sub_id: &str, customer_id: &str, status: DomainStatus) -> RawSubscription {
    RawSubscription {
        subscription_id: sub_id.to_string(),
        customer_id: customer_id.to_string(),
        price_id: Some("price_test".to_string()),
        status,
        current_period_start: None,
        current_period_end: None,
        cancel_at_period_end: false,
        canceled_at: None,
    }
}

fn checkout_event(
    created: DateTime<Utc>,
    customer_id: &str,
    subscription_id: &str,
    user_id: UserId,
) -> StripeEvent {
    StripeEvent {
        id: format!("evt_checkout_{}", created.timestamp()),
        type_: "checkout.session.completed".to_string(),
        created,
        kind: StripeEventKind::CheckoutSessionCompleted {
            customer_id: Some(customer_id.to_string()),
            subscription_id: Some(subscription_id.to_string()),
            user_id_metadata: Some(user_id),
        },
    }
}

fn subscription_event(kind: StripeEventKind, created: DateTime<Utc>, label: &str) -> StripeEvent {
    StripeEvent {
        id: format!("evt_{label}_{}", created.timestamp()),
        type_: label.to_string(),
        created,
        kind,
    }
}

async fn read_subscription(
    app: &TestedApp,
    billing: &Arc<BillingService>,
    user_id: UserId,
) -> Option<universal_inbox::billing::UserSubscription> {
    let mut transaction = app.repository.begin().await.unwrap();
    let sub = app
        .repository
        .get_user_subscription(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let _ = billing;
    sub
}

async fn seed_validated(
    app: &TestedApp,
    user_id: UserId,
    config: IntegrationConnectionConfig,
) -> Box<IntegrationConnection> {
    crate::helpers::integration_connection::create_integration_connection(
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

/// Distinct provider kinds (one per kind — the table is unique on
/// `(user_id, provider_kind)`) whose configs all enable notification sync, so a
/// single `is_sync_notifications_enabled()` assertion works across them.
fn notification_sync_configs() -> Vec<IntegrationConnectionConfig> {
    vec![
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled()),
    ]
}

async fn fetch_all_connections(app: &TestedApp, user_id: UserId) -> Vec<IntegrationConnection> {
    let mut transaction = app.repository.begin().await.unwrap();
    let connections = app
        .repository
        .fetch_all_integration_connections(&mut transaction, user_id, None, false)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    connections
}

async fn set_grace_deadline(app: &TestedApp, user_id: UserId, deadline: Option<DateTime<Utc>>) {
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .set_over_limit_grace_deadline(&mut transaction, user_id, deadline)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn reconcile(app: &TestedApp, billing: &Arc<BillingService>) -> ReconcileReport {
    let _ = app;
    billing.reconcile_subscriptions(false).await.unwrap()
}

async fn apply(
    app: &TestedApp,
    billing: &Arc<BillingService>,
    event: &StripeEvent,
    prefetched: Option<RawSubscription>,
) {
    let mut transaction = app.repository.begin().await.unwrap();
    billing
        .apply_subscription_event(&mut transaction, event, prefetched)
        .await
        .expect("apply_subscription_event should succeed");
    transaction.commit().await.unwrap();
}

/// Boot a `BillingService` against the test database + a stubbed
/// `StripeClient` that never calls Stripe. Used to exercise the routes
/// without external dependencies.
fn build_test_billing_service(
    app: &TestedApp,
) -> Arc<universal_inbox_api::billing::service::BillingService> {
    build_test_billing_service_with_customer_subscriptions(app, vec![])
}

/// Same, with a stubbed answer to "what does Stripe hold for this customer?" —
/// the question the reconcile job asks when a row has no subscription id.
fn build_test_billing_service_with_customer_subscriptions(
    app: &TestedApp,
    customer_subscriptions: Vec<RawSubscription>,
) -> Arc<universal_inbox_api::billing::service::BillingService> {
    use universal_inbox_api::billing::{
        service::{BillingService, RepositoryIntegrationCounter},
        stripe::{
            CheckoutSessionParams, PortalSessionParams, RawSubscription, StripeClient, StripeError,
            StripeEvent,
        },
    };

    // Minimal stub: production code paths under test never reach the wire,
    // so each method short-circuits with a deterministic value.
    // `customer_subscriptions` is what Stripe "holds" for any customer — the
    // reconcile adoption path is the only reader.
    struct NoopStripeClient {
        customer_subscriptions: Vec<RawSubscription>,
    }
    #[async_trait::async_trait]
    impl StripeClient for NoopStripeClient {
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
                "fetch_subscription is not exercised by these tests"
            )))
        }
        async fn list_customer_subscriptions(
            &self,
            _customer_id: &str,
        ) -> Result<Vec<RawSubscription>, StripeError> {
            Ok(self.customer_subscriptions.clone())
        }
        fn verify_webhook_signature(
            &self,
            _payload: &str,
            _signature_header: &str,
        ) -> Result<StripeEvent, StripeError> {
            Err(StripeError::InvalidWebhookSignature)
        }
    }

    let free_plan = universal_inbox_api::configuration::FreePlanSettings::default();
    let stripe: Arc<dyn StripeClient> = Arc::new(NoopStripeClient {
        customer_subscriptions,
    });
    let counter = Arc::new(RepositoryIntegrationCounter {
        repository: app.repository.clone(),
    });
    Arc::new(BillingService::new(
        app.repository.clone(),
        stripe,
        &free_plan,
        universal_inbox::billing::StripePriceId("price_test".to_string()),
        counter,
    ))
}

/// Self-hosting promise: when
/// `[billing]` is absent from configuration, every `/api/billing/*`
/// endpoint must respond `404 Not Found` so the web app treats billing as
/// disabled and the operator never has to set up Stripe. The test suite
/// boots without `[billing]` (see `api/config/test.toml`), so this is the
/// default state.
#[rstest]
#[tokio::test]
async fn billing_endpoints_404_when_billing_is_disabled(#[future] tested_app: TestedApp) {
    let app = tested_app.await;
    let client = reqwest::Client::new();

    // GET /api/billing/me must 404, not 401 — the route doesn't exist at all
    // on a self-hosted instance, so authentication is moot.
    let response = client
        .get(format!("{}billing/me", app.api_address))
        .send()
        .await
        .expect("Failed to call /api/billing/me");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // POST /api/billing/checkout-session must 404 for the same reason.
    let response = client
        .post(format!("{}billing/checkout-session", app.api_address))
        .json(&json!({
            "success_url": "https://example.test/billing?checkout=success",
            "cancel_url": "https://example.test/billing?checkout=cancel"
        }))
        .send()
        .await
        .expect("Failed to call /api/billing/checkout-session");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // POST /api/billing/portal-session ditto.
    let response = client
        .post(format!("{}billing/portal-session", app.api_address))
        .json(&json!({ "return_url": "https://example.test/billing" }))
        .send()
        .await
        .expect("Failed to call /api/billing/portal-session");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // POST /api/billing/stripe/webhook ditto. Even a forged Stripe payload
    // is rejected at the routing layer rather than the signature layer,
    // because the route itself does not exist when billing is disabled.
    let response = client
        .post(format!("{}billing/stripe/webhook", app.api_address))
        .header("Stripe-Signature", "t=1,v1=deadbeef")
        .body(
            r#"{"id":"evt_test","object":"event","type":"customer.created","data":{"object":{}}}"#,
        )
        .send()
        .await
        .expect("Failed to call /api/billing/stripe/webhook");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Direct service-level test of plan defaults + sync interval resolution
/// when billing is wired up via a stubbed `StripeClient`: a Free user's
/// effective interval is max(global, plan_floor), a Paid user gets the global
/// value unchanged.
#[rstest]
#[tokio::test]
async fn billing_service_resolves_sync_intervals_with_a_mocked_stripe_client(
    #[future] tested_app: TestedApp,
) {
    use universal_inbox::billing::Plan;
    use universal_inbox_api::billing::service::{SyncKind, effective_sync_interval};

    let app = tested_app.await;
    let billing = build_test_billing_service(&app);

    // Free defaults: 1440 (once a day) for both notifications and tasks.
    assert_eq!(
        effective_sync_interval(billing.limits(), Plan::Free, 60, SyncKind::Notifications),
        1440,
        "Free plan floor (1440) wins over global 60"
    );
    assert_eq!(
        effective_sync_interval(billing.limits(), Plan::Free, 2880, SyncKind::Notifications),
        2880,
        "Global floor (2880) wins over Free plan 1440"
    );
    assert_eq!(
        effective_sync_interval(billing.limits(), Plan::Paid, 5, SyncKind::Tasks),
        5,
        "Paid plan uses the global value unchanged"
    );
}

/// A paying customer with no pre-existing local row must still be linked to
/// their plan. `upsert_from_raw` resolves the user through existing rows and
/// falls back to the `metadata.user_id` the event carries, so a checkout
/// completion whose customer matches no row still lands: Stripe is handed an
/// idempotency record and never retries, and the charged user would otherwise
/// stay on Free.
#[rstest]
#[tokio::test]
async fn checkout_completion_links_user_via_metadata_fallback(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "b1@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    // No prior user_subscription row exists for this user/customer.
    assert!(read_subscription(&app, &billing, user.id).await.is_none());

    // checkout.session.completed with an active subscription that was
    // prefetched outside the transaction by the webhook handler.
    let event = checkout_event(ts(1_700_000_000), "cus_b1", "sub_b1", user.id);
    let prefetched = raw_sub("sub_b1", "cus_b1", DomainStatus::Active);
    apply(&app, &billing, &event, Some(prefetched)).await;

    let sub = read_subscription(&app, &billing, user.id)
        .await
        .expect("row must be created from the metadata fallback");
    assert_eq!(sub.user_id, user.id);
    assert_eq!(
        sub.stripe_customer_id.as_ref().map(|c| c.as_str()),
        Some("cus_b1")
    );
    assert_eq!(
        sub.stripe_subscription_id.as_ref().map(|s| s.as_str()),
        Some("sub_b1")
    );
    assert_eq!(
        sub.effective_plan(),
        Plan::Paid,
        "charged customer must be Paid"
    );
}

/// Stripe does not guarantee delivery order. A stale retried
/// `subscription.updated(active)` arriving after `subscription.deleted` must
/// NOT resurrect the canceled subscription. The ordering guard compares
/// `event.created` against the row's `last_stripe_event_at`.
#[rstest]
#[tokio::test]
async fn stale_subscription_event_does_not_resurrect_canceled_sub(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "b2@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    // Seed an active subscription via checkout completion at T0.
    apply(
        &app,
        &billing,
        &checkout_event(ts(1_700_000_000), "cus_b2", "sub_b2", user.id),
        Some(raw_sub("sub_b2", "cus_b2", DomainStatus::Active)),
    )
    .await;

    // subscription.deleted at T2 (later) → canceled.
    apply(
        &app,
        &billing,
        &subscription_event(
            StripeEventKind::SubscriptionDeleted(raw_sub(
                "sub_b2",
                "cus_b2",
                DomainStatus::Canceled,
            )),
            ts(1_700_000_200),
            "customer.subscription.deleted",
        ),
        None,
    )
    .await;
    assert_eq!(
        read_subscription(&app, &billing, user.id)
            .await
            .unwrap()
            .status,
        DomainStatus::Canceled
    );

    // Stale subscription.updated(active) at T1 (< T2) → must be ignored.
    apply(
        &app,
        &billing,
        &subscription_event(
            StripeEventKind::SubscriptionUpserted(raw_sub(
                "sub_b2",
                "cus_b2",
                DomainStatus::Active,
            )),
            ts(1_700_000_100),
            "customer.subscription.updated",
        ),
        None,
    )
    .await;
    let sub = read_subscription(&app, &billing, user.id).await.unwrap();
    assert_eq!(
        sub.status,
        DomainStatus::Canceled,
        "stale event must not resurrect the canceled subscription"
    );
    assert_eq!(sub.effective_plan(), Plan::Free);

    // A genuinely newer subscription.updated(active) at T3 (> T2) still applies.
    apply(
        &app,
        &billing,
        &subscription_event(
            StripeEventKind::SubscriptionUpserted(raw_sub(
                "sub_b2",
                "cus_b2",
                DomainStatus::Active,
            )),
            ts(1_700_000_300),
            "customer.subscription.updated",
        ),
        None,
    )
    .await;
    assert_eq!(
        read_subscription(&app, &billing, user.id)
            .await
            .unwrap()
            .status,
        DomainStatus::Active,
        "a newer event must still apply"
    );
}

/// The service's ordering guard compares against the row it read, which two
/// concurrent webhooks both pass, so the write must carry the guard too.
/// Driving the repository directly is what a lost race looks like at the sink:
/// a write built from state that was current when it was read.
#[rstest]
#[tokio::test]
async fn a_raced_stale_subscription_write_cannot_overwrite_newer_state(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "b4@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    apply(
        &app,
        &billing,
        &checkout_event(ts(1_700_000_000), "cus_b4", "sub_b4", user.id),
        Some(raw_sub("sub_b4", "cus_b4", DomainStatus::Active)),
    )
    .await;
    apply(
        &app,
        &billing,
        &subscription_event(
            StripeEventKind::SubscriptionDeleted(raw_sub(
                "sub_b4",
                "cus_b4",
                DomainStatus::Canceled,
            )),
            ts(1_700_000_200),
            "customer.subscription.deleted",
        ),
        None,
    )
    .await;
    let canceled = read_subscription(&app, &billing, user.id).await.unwrap();
    assert_eq!(canceled.status, DomainStatus::Canceled);

    // The losing writer: an older event carrying paid/active, built before the
    // cancellation committed.
    let stale = universal_inbox::billing::UserSubscription {
        status: DomainStatus::Active,
        plan: Plan::Paid,
        canceled_at: None,
        last_stripe_event_at: Some(ts(1_700_000_100)),
        ..canceled.clone()
    };
    let mut transaction = app.repository.begin().await.unwrap();
    let returned = app
        .repository
        .upsert_user_subscription(&mut transaction, &stale)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert_eq!(
        returned.status,
        DomainStatus::Canceled,
        "the skipped write must report the state actually in force"
    );
    let stored = read_subscription(&app, &billing, user.id).await.unwrap();
    assert_eq!(
        stored.status,
        DomainStatus::Canceled,
        "a stale write must not resurrect the canceled subscription"
    );
    assert_eq!(stored.effective_plan(), Plan::Free);
    assert_eq!(
        stored.last_stripe_event_at, canceled.last_stripe_event_at,
        "nor roll the ordering guard's own watermark backwards"
    );

    // The same sink still accepts a genuinely newer event.
    let newer = universal_inbox::billing::UserSubscription {
        status: DomainStatus::Active,
        plan: Plan::Paid,
        last_stripe_event_at: Some(ts(1_700_000_300)),
        ..canceled.clone()
    };
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .upsert_user_subscription(&mut transaction, &newer)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(
        read_subscription(&app, &billing, user.id)
            .await
            .unwrap()
            .status,
        DomainStatus::Active,
        "a newer write must still apply"
    );
}

/// `apply_subscription_event` must consume the subscription state the
/// webhook handler prefetched *outside* the transaction rather than fetching
/// from Stripe itself. The stubbed `StripeClient::fetch_subscription` errors,
/// so a successful apply proves no in-transaction Stripe call happened.
#[rstest]
#[tokio::test]
async fn invoice_paid_uses_prefetched_state_without_fetching(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "b3@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    // Seed a subscription row so the invoice resolves to a user.
    apply(
        &app,
        &billing,
        &checkout_event(ts(1_700_000_000), "cus_b3", "sub_b3", user.id),
        Some(raw_sub("sub_b3", "cus_b3", DomainStatus::PastDue)),
    )
    .await;

    // invoice.paid with prefetched active state. NoopStripeClient's
    // fetch_subscription returns Err — if apply tried to fetch, this panics.
    let event = subscription_event(
        StripeEventKind::InvoicePaid {
            customer_id: Some("cus_b3".to_string()),
            subscription_id: Some("sub_b3".to_string()),
        },
        ts(1_700_000_400),
        "invoice.paid",
    );
    apply(
        &app,
        &billing,
        &event,
        Some(raw_sub("sub_b3", "cus_b3", DomainStatus::Active)),
    )
    .await;

    assert_eq!(
        read_subscription(&app, &billing, user.id)
            .await
            .unwrap()
            .status,
        DomainStatus::Active
    );
}

/// Cluster A end-to-end: a Paid user over the integration cap downgrades, the
/// grace clock arms (connections untouched), the reconcile job pauses the
/// excess after the deadline (config off + marker + snapshot), and an upgrade
/// restores the exact pre-pause config and clears the markers. Also proves the
/// A4 deepening: the grace deadline fires even though the downgraded row is in
/// the terminal `canceled` status that the reconciliation refresh query skips.
#[rstest]
#[tokio::test]
async fn downgrade_arms_grace_then_reconcile_pauses_then_upgrade_restores(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "a-cycle@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    // One more validated connection than the Free plan allows (distinct kinds).
    let limit = billing.limits().max_integration_connections as usize;
    let configs = notification_sync_configs();
    assert!(
        configs.len() > limit,
        "test needs at least limit+1 distinct provider configs"
    );
    for config in configs.into_iter().take(limit + 1) {
        seed_validated(&app, user.id, config).await;
    }

    // Become Paid via checkout.
    apply(
        &app,
        &billing,
        &checkout_event(ts(1_700_000_000), "cus_a", "sub_a", user.id),
        Some(raw_sub("sub_a", "cus_a", DomainStatus::Active)),
    )
    .await;
    assert_eq!(
        read_subscription(&app, &billing, user.id)
            .await
            .unwrap()
            .effective_plan(),
        Plan::Paid
    );

    // Downgrade by deletion → Free. Grace arms; connections stay enabled.
    apply(
        &app,
        &billing,
        &subscription_event(
            StripeEventKind::SubscriptionDeleted(raw_sub("sub_a", "cus_a", DomainStatus::Canceled)),
            ts(1_700_000_200),
            "customer.subscription.deleted",
        ),
        None,
    )
    .await;
    let sub = read_subscription(&app, &billing, user.id).await.unwrap();
    assert_eq!(sub.effective_plan(), Plan::Free);
    assert_eq!(sub.status, DomainStatus::Canceled);
    assert!(
        sub.over_limit_grace_deadline.is_some(),
        "Paid→Free over the cap must arm the grace deadline"
    );
    for connection in fetch_all_connections(&app, user.id).await {
        assert!(
            connection.provider.is_sync_notifications_enabled(),
            "connections must stay enabled during the grace window"
        );
        assert!(connection.auto_paused_by_plan_at.is_none());
    }

    // Expire the deadline and reconcile. The row is `canceled` (excluded from
    // the refresh query) yet its grace deadline must still fire.
    set_grace_deadline(&app, user.id, Some(ts(1))).await;
    let grace_expired = reconcile(&app, &billing).await.grace_expired;
    assert_eq!(
        grace_expired, 1,
        "the canceled row's expired grace must enforce"
    );

    let connections = fetch_all_connections(&app, user.id).await;
    let paused: Vec<_> = connections
        .iter()
        .filter(|c| c.auto_paused_by_plan_at.is_some())
        .collect();
    assert_eq!(
        paused.len(),
        1,
        "exactly the one over-limit connection is paused"
    );
    assert!(
        !paused[0].provider.is_sync_notifications_enabled(),
        "paused connection has its sync toggled off"
    );
    assert!(
        paused[0].auto_paused_config_snapshot.is_some(),
        "paused connection stores its pre-pause config snapshot"
    );
    assert_eq!(
        connections
            .iter()
            .filter(|c| c.provider.is_sync_notifications_enabled())
            .count(),
        limit,
        "the rest stay enabled, exactly up to the limit"
    );

    // Enforcement must *settle* the over-limit state. A paused connection
    // consumes no slot, so usage is back at the cap and a `/billing/me` read
    // (which lazily arms the deadline) must not start a fresh countdown —
    // otherwise the banner re-arms every page load and never goes away.
    let mut transaction = app.repository.begin().await.unwrap();
    let used = app
        .repository
        .count_validated_integration_connections(&mut transaction, user.id)
        .await
        .unwrap();
    let paused_count = app
        .repository
        .count_plan_paused_integration_connections(&mut transaction, user.id)
        .await
        .unwrap();
    let re_armed = billing
        .mark_over_limit_grace_deadline_if_needed(&mut transaction, user.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(used as usize, limit, "paused connections free their slots");
    assert_eq!(paused_count, 1, "the paused one is reported separately");
    assert_eq!(re_armed, None, "a settled over-limit state does not re-arm");
    assert!(
        read_subscription(&app, &billing, user.id)
            .await
            .unwrap()
            .over_limit_grace_deadline
            .is_none(),
        "no grace deadline survives enforcement"
    );

    // Upgrade → restore. Deadline cleared, markers + snapshots cleared, and the
    // paused connection's config is restored to its enabled pre-pause state.
    apply(
        &app,
        &billing,
        &subscription_event(
            StripeEventKind::SubscriptionUpserted(raw_sub("sub_a", "cus_a", DomainStatus::Active)),
            ts(1_700_000_400),
            "customer.subscription.updated",
        ),
        None,
    )
    .await;
    let sub = read_subscription(&app, &billing, user.id).await.unwrap();
    assert_eq!(sub.effective_plan(), Plan::Paid);
    assert!(
        sub.over_limit_grace_deadline.is_none(),
        "upgrade clears the grace deadline"
    );
    for connection in fetch_all_connections(&app, user.id).await {
        assert!(
            connection.auto_paused_by_plan_at.is_none(),
            "marker cleared on upgrade"
        );
        assert!(
            connection.auto_paused_config_snapshot.is_none(),
            "snapshot cleared on upgrade"
        );
        assert!(
            connection.provider.is_sync_notifications_enabled(),
            "config restored to its pre-pause enabled state on upgrade"
        );
    }
}

/// The `SELECT COUNT(*)` path returns the same number as the old
/// "load every row + JSON config, then `.len()`" path for real integrations.
/// (They diverge only when an auto-created `API` connection exists — covered by
/// `count_validated_excludes_api_connection` below.)
#[rstest]
#[tokio::test]
async fn count_validated_matches_fetch_all_len(#[future] tested_app_with_local_auth: TestedApp) {
    let app = tested_app_with_local_auth.await;
    let user = crate::helpers::user::create_user(
        &app,
        "count@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    let configs = notification_sync_configs();
    let expected = configs.len() as u32;
    for config in configs {
        seed_validated(&app, user.id, config).await;
    }

    let mut transaction = app.repository.begin().await.unwrap();
    let counted = app
        .repository
        .count_validated_integration_connections(&mut transaction, user.id)
        .await
        .unwrap();
    let via_fetch = app
        .repository
        .fetch_all_integration_connections(
            &mut transaction,
            user.id,
            Some(IntegrationConnectionStatus::Validated),
            false,
        )
        .await
        .unwrap()
        .len() as u32;
    transaction.commit().await.unwrap();

    assert_eq!(
        counted, via_fetch,
        "COUNT(*) must match the loaded-rows count"
    );
    assert_eq!(counted, expected);
}

/// The `API` provider kind is auto-created on first programmatic API use and is
/// never a user-facing integration, so it must not count toward billing usage
/// or the connection cap. Regression for the header banner / billing page
/// reporting "2/2" when a user has a single real integration plus the implicit
/// `API` connection.
#[rstest]
#[tokio::test]
async fn count_validated_excludes_api_connection(#[future] tested_app_with_local_auth: TestedApp) {
    let app = tested_app_with_local_auth.await;
    let user = crate::helpers::user::create_user(
        &app,
        "api-exclude@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    // One real integration + the implicit API connection, both Validated.
    seed_validated(
        &app,
        user.id,
        IntegrationConnectionConfig::Todoist(Default::default()),
    )
    .await;
    seed_validated(&app, user.id, IntegrationConnectionConfig::API).await;

    let mut transaction = app.repository.begin().await.unwrap();
    let counted = app
        .repository
        .count_validated_integration_connections(&mut transaction, user.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert_eq!(
        counted, 1,
        "the implicit API connection must not count toward billing usage"
    );
}

/// The reconcile cron prunes `stripe_event` rows older than the retention
/// window and leaves fresher ones; `--dry-run` deletes nothing.
#[rstest]
#[tokio::test]
async fn prune_stripe_events_removes_only_old_rows(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);

    // One row well past the 30-day window, one fresh.
    let mut tx = app.repository.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO stripe_event (event_id, event_type, received_at) \
         VALUES ($1, 'test', now() - interval '40 days')",
    )
    .bind("evt_old")
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO stripe_event (event_id, event_type, received_at) VALUES ($1, 'test', now())",
    )
    .bind("evt_new")
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    // Dry-run must not delete anything.
    let mut tx = app.repository.begin().await.unwrap();
    let pruned_dry = billing.prune_stripe_events(&mut tx, true).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(pruned_dry, 0, "--dry-run prunes nothing");

    // Real run deletes exactly the 40-day-old row.
    let mut tx = app.repository.begin().await.unwrap();
    let pruned = billing.prune_stripe_events(&mut tx, false).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(pruned, 1, "exactly the stale row is pruned");

    let mut tx = app.repository.begin().await.unwrap();
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stripe_event WHERE event_id IN ('evt_old', 'evt_new')",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(remaining, 1, "the fresh row survives");
}

/// `mark_over_limit_grace_deadline_if_needed` arms a deadline for an
/// over-limit Free user with no Stripe row (placeholder), and is idempotent on
/// re-call. The placeholder uses `Incomplete` status so the grace-expiry pass
/// can still reach it.
#[rstest]
#[tokio::test]
async fn over_limit_free_user_arms_grace_idempotently_and_is_reconcilable(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "a1@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    let limit = billing.limits().max_integration_connections as usize;
    let configs = notification_sync_configs();
    assert!(configs.len() > limit);
    for config in configs.into_iter().take(limit + 1) {
        seed_validated(&app, user.id, config).await;
    }
    assert!(read_subscription(&app, &billing, user.id).await.is_none());

    let first = {
        let mut transaction = app.repository.begin().await.unwrap();
        let deadline = billing
            .mark_over_limit_grace_deadline_if_needed(&mut transaction, user.id)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        deadline
    };
    assert!(
        first.is_some(),
        "over-limit Free user gets a grace deadline armed"
    );
    let second = {
        let mut transaction = app.repository.begin().await.unwrap();
        let deadline = billing
            .mark_over_limit_grace_deadline_if_needed(&mut transaction, user.id)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        deadline
    };
    assert_eq!(
        second, first,
        "re-arming is idempotent — deadline unchanged"
    );

    let placeholder = read_subscription(&app, &billing, user.id).await.unwrap();
    assert_eq!(
        placeholder.status,
        DomainStatus::Incomplete,
        "placeholder uses Incomplete (not Canceled) so reconcile can reach it"
    );
    assert_eq!(placeholder.effective_plan(), Plan::Free);

    // Expire and reconcile — the placeholder's deadline must enforce.
    set_grace_deadline(&app, user.id, Some(ts(1))).await;
    let grace_expired = reconcile(&app, &billing).await.grace_expired;
    assert_eq!(grace_expired, 1, "placeholder row's expired grace enforces");
    let paused = fetch_all_connections(&app, user.id)
        .await
        .into_iter()
        .filter(|c| c.auto_paused_by_plan_at.is_some())
        .count();
    assert_eq!(paused, 1);
}

/// Once a Free user is back at/under the cap, `mark_over_limit_grace_deadline_if_needed`
/// must clear a previously-armed deadline so the over-limit banner stops
/// showing. Regression for the banner persisting after the usage count drops.
#[rstest]
#[tokio::test]
async fn returning_under_cap_clears_armed_grace_deadline(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "disarm@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    let limit = billing.limits().max_integration_connections as usize;
    let configs = notification_sync_configs();
    assert!(configs.len() > limit);
    let mut seeded = vec![];
    for config in configs.into_iter().take(limit + 1) {
        seeded.push(seed_validated(&app, user.id, config).await);
    }

    // Arm the deadline while over the cap.
    {
        let mut transaction = app.repository.begin().await.unwrap();
        let armed = billing
            .mark_over_limit_grace_deadline_if_needed(&mut transaction, user.id)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert!(armed.is_some(), "over-limit user arms a deadline");
    }

    // Drop one connection out of `Validated` so the user is back at the cap.
    {
        let mut transaction = app.repository.begin().await.unwrap();
        app.repository
            .update_integration_connection_status(
                &mut transaction,
                seeded[0].id,
                IntegrationConnectionStatus::Created,
                None,
                None,
                user.id,
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();
    }

    // Next read must clear the stale deadline.
    {
        let mut transaction = app.repository.begin().await.unwrap();
        let result = billing
            .mark_over_limit_grace_deadline_if_needed(&mut transaction, user.id)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(result, None, "no deadline returned once under the cap");
    }

    let sub = read_subscription(&app, &billing, user.id).await.unwrap();
    assert_eq!(
        sub.over_limit_grace_deadline, None,
        "stale grace deadline cleared once back under the cap"
    );
}

/// A checkout whose `checkout.session.completed` webhook never reaches the
/// instance (dropped delivery, no local forwarder, endpoint down) leaves a row
/// that knows the customer but not the subscription, and the user is charged
/// while staying on Free. Reconcile must ask Stripe what that customer holds
/// and adopt the live subscription.
#[rstest]
#[tokio::test]
async fn reconcile_adopts_the_subscription_of_a_missed_checkout_webhook(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let seeding_billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "adopt@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    // The customer-only row a checkout leaves behind before the subscription
    // webhook lands.
    apply(
        &app,
        &seeding_billing,
        &StripeEvent {
            id: "evt_checkout_no_subscription".to_string(),
            type_: "checkout.session.completed".to_string(),
            created: ts(1_700_000_000),
            kind: StripeEventKind::CheckoutSessionCompleted {
                customer_id: Some("cus_adopt".to_string()),
                subscription_id: None,
                user_id_metadata: Some(user.id),
            },
        },
        None,
    )
    .await;
    let sub = read_subscription(&app, &seeding_billing, user.id)
        .await
        .expect("checkout must record the customer linkage");
    assert!(sub.stripe_subscription_id.is_none());
    assert_eq!(sub.effective_plan(), Plan::Free);

    // Stripe, meanwhile, holds the paid subscription nobody told us about.
    let billing = build_test_billing_service_with_customer_subscriptions(
        &app,
        vec![raw_sub("sub_adopt", "cus_adopt", DomainStatus::Active)],
    );
    let report = reconcile(&app, &billing).await;
    assert_eq!(report.adopted, 1, "the live subscription must be adopted");
    assert_eq!(report.refreshed, 0, "no row carried a subscription id");

    let sub = read_subscription(&app, &billing, user.id).await.unwrap();
    assert_eq!(
        sub.stripe_subscription_id.as_ref().map(|s| s.as_str()),
        Some("sub_adopt")
    );
    assert_eq!(
        sub.effective_plan(),
        Plan::Paid,
        "the charged user must end up Paid without ever seeing the webhook"
    );
}

/// Adoption only takes a subscription that is actually live. A customer whose
/// history holds nothing but terminal subscriptions (they canceled, and the
/// cancellation webhook is what went missing) must stay on Free rather than be
/// handed a plan they no longer pay for.
#[rstest]
#[tokio::test]
async fn reconcile_does_not_adopt_a_terminal_subscription(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let seeding_billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "no-adopt@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    apply(
        &app,
        &seeding_billing,
        &StripeEvent {
            id: "evt_checkout_no_subscription_2".to_string(),
            type_: "checkout.session.completed".to_string(),
            created: ts(1_700_000_000),
            kind: StripeEventKind::CheckoutSessionCompleted {
                customer_id: Some("cus_no_adopt".to_string()),
                subscription_id: None,
                user_id_metadata: Some(user.id),
            },
        },
        None,
    )
    .await;

    let billing = build_test_billing_service_with_customer_subscriptions(
        &app,
        vec![raw_sub(
            "sub_no_adopt",
            "cus_no_adopt",
            DomainStatus::Canceled,
        )],
    );
    let report = reconcile(&app, &billing).await;
    assert_eq!(report.adopted, 0, "a canceled subscription is not adopted");

    let sub = read_subscription(&app, &billing, user.id).await.unwrap();
    assert!(sub.stripe_subscription_id.is_none());
    assert_eq!(sub.effective_plan(), Plan::Free);
}

/// A pause enforces the cap by switching sync toggles off, so a provider kind
/// with no toggle cannot be paused: writing the marker anyway would stop the
/// connection counting against the cap (paused rows are excluded from the
/// usage count) while it kept working, and the freed slot would clear the
/// grace deadline too. Such a connection must stay counted, and the cap must
/// be enforced on a connection that can actually be restricted.
#[rstest]
#[tokio::test]
async fn a_connection_a_pause_cannot_restrict_keeps_counting_against_the_cap(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "b5@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    let limit = billing.limits().max_integration_connections as usize;
    let mut togglable = vec![];
    for config in notification_sync_configs().into_iter().take(limit) {
        togglable.push(seed_validated(&app, user.id, config).await.id);
    }
    // Newest connection, and the one a pause cannot touch.
    let calendar = seed_validated(
        &app,
        user.id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
    )
    .await
    .id;

    let mut transaction = app.repository.begin().await.unwrap();
    let paused = billing
        .enforce_free_plan_compliance(&mut transaction, user.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    // Exactly the one excess connection is paused, and it is one a pause can
    // actually restrict. Which of the togglable ones it is depends on
    // `created_at`, which the fixtures can write within the same instant.
    assert_eq!(paused.len(), 1, "exactly the excess is paused");
    assert!(
        togglable.contains(&paused[0]),
        "the paused connection must be one a pause can restrict"
    );
    assert!(
        !paused.contains(&calendar),
        "a connection with no sync toggle must not be marked paused"
    );

    let connections = fetch_all_connections(&app, user.id).await;
    let calendar_connection = connections
        .iter()
        .find(|connection| connection.id == calendar)
        .expect("the calendar connection still exists");
    assert!(
        calendar_connection.auto_paused_by_plan_at.is_none(),
        "no marker, so it keeps counting against the cap"
    );

    let mut transaction = app.repository.begin().await.unwrap();
    let used = app
        .repository
        .count_validated_integration_connections(&mut transaction, user.id)
        .await
        .unwrap();
    let re_armed = billing
        .mark_over_limit_grace_deadline_if_needed(&mut transaction, user.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert_eq!(
        used as usize, limit,
        "usage counts the calendar connection and the surviving togglable ones"
    );
    assert_eq!(
        re_armed, None,
        "enforcement settled the over-limit state, so nothing re-arms"
    );
}

/// Re-pausing an already-paused connection would capture the disabled config
/// as its snapshot, which `restore_plan_paused_connections` writes back on
/// upgrade — leaving a paying user switched off.
#[rstest]
#[tokio::test]
async fn enforcement_run_twice_keeps_the_pre_pause_snapshot(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "b6@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    let limit = billing.limits().max_integration_connections as usize;
    for config in notification_sync_configs().into_iter().take(limit + 1) {
        seed_validated(&app, user.id, config).await;
    }

    let mut transaction = app.repository.begin().await.unwrap();
    let first = billing
        .enforce_free_plan_compliance(&mut transaction, user.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(first.len(), 1);
    let paused_id = first[0];

    let paused_connection = fetch_all_connections(&app, user.id)
        .await
        .into_iter()
        .find(|connection| connection.id == paused_id)
        .expect("the paused connection exists");
    let snapshot_after_first = paused_connection
        .auto_paused_config_snapshot
        .clone()
        .expect("the paused connection stores its pre-pause config");
    assert_ne!(
        snapshot_after_first,
        paused_connection.provider.config(),
        "the snapshot must capture the config as the user had it, not the disabled one"
    );

    // A second pass — reached by the re-arm loop when some connections cannot
    // be paused, or by two overlapping reconcile runs.
    let mut transaction = app.repository.begin().await.unwrap();
    let second = billing
        .enforce_free_plan_compliance(&mut transaction, user.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert!(
        second.is_empty(),
        "nothing is over the cap any more, so nothing is paused again"
    );
    let snapshot_after_second = fetch_all_connections(&app, user.id)
        .await
        .into_iter()
        .find(|connection| connection.id == paused_id)
        .and_then(|connection| connection.auto_paused_config_snapshot)
        .expect("the paused connection still stores a snapshot");
    assert_eq!(
        snapshot_after_second, snapshot_after_first,
        "a second pass must not overwrite the snapshot with the disabled config"
    );
}

/// The excess must be measured the way the cap is: against connections that
/// still count. A paused one consumes no slot, so counting it as excess pauses
/// one connection too many.
#[rstest]
#[tokio::test]
async fn enforcement_does_not_pause_a_user_already_at_the_cap(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "b7@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    let limit = billing.limits().max_integration_connections as usize;
    for config in notification_sync_configs().into_iter().take(limit) {
        seed_validated(&app, user.id, config).await;
    }
    // A connection an older enforcement marked paused. It consumes no slot, so
    // the user is exactly at the cap.
    let legacy_paused = seed_validated(
        &app,
        user.id,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
    )
    .await
    .id;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .set_integration_connection_plan_pause(
            &mut transaction,
            legacy_paused,
            Some(Utc::now()),
            Some(&IntegrationConnectionConfig::GoogleCalendar(
                GoogleCalendarConfig::enabled(),
            )),
        )
        .await
        .unwrap();
    let used_before = app
        .repository
        .count_validated_integration_connections(&mut transaction, user.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(used_before as usize, limit, "the user starts at the cap");

    let mut transaction = app.repository.begin().await.unwrap();
    let paused = billing
        .enforce_free_plan_compliance(&mut transaction, user.id)
        .await
        .unwrap();
    let used_after = app
        .repository
        .count_validated_integration_connections(&mut transaction, user.id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert!(
        paused.is_empty(),
        "a user at the cap must keep every counting connection, got {paused:?}"
    );
    assert_eq!(used_after as usize, limit);
}

/// The implicit `API` connection (browser-extension capture) is excluded from
/// the count the cap is measured against, so the cap must not refuse it
/// either.
#[rstest]
#[tokio::test]
async fn the_cap_does_not_refuse_the_implicit_api_connection(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let billing = build_test_billing_service(&app);
    let user = crate::helpers::user::create_user(
        &app,
        "b8@billing.test".parse().unwrap(),
        "correct horse battery staple",
    )
    .await;

    let limit = billing.limits().max_integration_connections as usize;
    for config in notification_sync_configs().into_iter().take(limit) {
        seed_validated(&app, user.id, config).await;
    }

    let mut transaction = app.repository.begin().await.unwrap();
    let api_verdict = billing
        .assert_can_add_integration(
            &mut transaction,
            user.id,
            universal_inbox::integration_connection::provider::IntegrationProviderKind::API,
        )
        .await;
    transaction.commit().await.unwrap();
    assert!(
        api_verdict.is_ok(),
        "the API connection consumes no slot, so the cap must allow it: {api_verdict:?}"
    );

    // A real provider at the same cap is still refused.
    let mut transaction = app.repository.begin().await.unwrap();
    let github_verdict = billing
        .assert_can_add_integration(
            &mut transaction,
            user.id,
            universal_inbox::integration_connection::provider::IntegrationProviderKind::Github,
        )
        .await;
    transaction.commit().await.unwrap();
    assert!(
        matches!(
            github_verdict,
            Err(universal_inbox_api::universal_inbox::UniversalInboxError::PaymentRequired { .. })
        ),
        "a counted provider at the cap must still be refused, got {github_verdict:?}"
    );
}
