use std::{str::FromStr, sync::Arc};

use chrono::{DateTime, Utc};
use cucumber::{Parameter, given, then};
use pretty_assertions::assert_eq;
use secrecy::SecretBox;
use uuid::Uuid;

use universal_inbox::{
    billing::{Plan, SubscriptionStatus, UserSubscription},
    integration_connection::IntegrationConnection,
};
use universal_inbox_api::{
    billing::{
        repository::BillingRepository,
        service::{BillingService, RepositoryIntegrationCounter},
        stripe::{
            CheckoutSessionParams, PortalSessionParams, RawSubscription, StripeClient, StripeError,
            StripeEvent, StripeEventKind, client::StripeApiClient,
        },
    },
    configuration::{FreePlanSettings, StripeApiKey, StripeWebhookSecret},
    repository::integration_connection::IntegrationConnectionRepository,
};

use crate::{
    helpers::{TestedApp, user::create_user},
    world::ApiWorld,
};

pub(super) const BASE: i64 = 1_700_000_000;

#[derive(Clone, Copy, Debug)]
pub struct StripeStatus(pub SubscriptionStatus);

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
pub struct PlanName(pub Plan);

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

pub(super) fn at(second: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(BASE + second, 0).expect("valid timestamp")
}

pub(super) fn raw_sub(
    subscription: &str,
    customer: &str,
    status: SubscriptionStatus,
) -> RawSubscription {
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

pub(super) fn service(
    app: &TestedApp,
    customer_subscriptions: Vec<RawSubscription>,
) -> Arc<BillingService> {
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

pub(super) async fn billing(world: &mut ApiWorld) -> (Arc<BillingService>, &TestedApp) {
    let holds = world.billing.stripe_holds.clone();
    let mock_uri = world.billing.stripe_mock.as_ref().map(|mock| mock.uri());
    let app = world.app.get().await;
    match mock_uri {
        Some(uri) => (service_against(app, &uri), app),
        None => (service(app, holds), app),
    }
}

pub(super) fn service_against(app: &TestedApp, stripe_uri: &str) -> Arc<BillingService> {
    let stripe = Arc::new(StripeApiClient::new_with_base_url(
        &SecretBox::new(Box::new(StripeApiKey("dummy-stripe-api-key".to_string()))),
        SecretBox::new(Box::new(StripeWebhookSecret(
            "dummy-webhook-secret".to_string(),
        ))),
        stripe_uri,
    ));
    Arc::new(BillingService::new(
        app.repository.clone(),
        stripe,
        &FreePlanSettings::default(),
        universal_inbox::billing::StripePriceId("price_test".to_string()),
        Arc::new(RepositoryIntegrationCounter {
            repository: app.repository.clone(),
        }),
    ))
}

pub(super) fn user_id(world: &ApiWorld) -> universal_inbox::user::UserId {
    world.billing.user_id.expect("No billing user exists")
}

pub(super) async fn limit(world: &mut ApiWorld) -> usize {
    let (billing, _) = billing(world).await;
    billing.limits().max_integration_connections as usize
}

pub(super) async fn read_subscription(world: &mut ApiWorld) -> Option<UserSubscription> {
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

pub(super) async fn connections(world: &mut ApiWorld) -> Vec<IntegrationConnection> {
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

pub(super) async fn apply(
    world: &mut ApiWorld,
    event: StripeEvent,
    prefetched: Option<RawSubscription>,
) {
    let (billing, app) = billing(world).await;
    let mut transaction = app.repository.begin().await.unwrap();
    billing
        .apply_subscription_event(&mut transaction, &event, prefetched)
        .await
        .expect("apply_subscription_event should succeed");
    transaction.commit().await.unwrap();
}

pub(super) fn event(second: i64, label: &str, kind: StripeEventKind) -> StripeEvent {
    StripeEvent {
        id: format!("evt_{label}_{second}"),
        type_: label.to_string(),
        created: at(second),
        kind,
    }
}

#[given("a user with a password exists")]
pub(super) async fn a_user_exists(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let user = create_user(
        app,
        format!("{}@billing.test", Uuid::new_v4()).parse().unwrap(),
        "correct horse battery staple",
    )
    .await;
    world.billing.user_id = Some(user.id);
}

#[then("the user has no subscription")]
pub(super) async fn the_user_has_no_subscription(world: &mut ApiWorld) {
    assert!(read_subscription(world).await.is_none());
}

#[then(expr = "the subscription of the user is {stripe_status}")]
pub(super) async fn the_subscription_is(world: &mut ApiWorld, status: StripeStatus) {
    assert_eq!(read_subscription(world).await.unwrap().status, status.0);
}

#[then(expr = "the user is on the {plan} plan")]
pub(super) async fn the_user_is_on_the_plan(world: &mut ApiWorld, plan: PlanName) {
    assert_eq!(
        read_subscription(world).await.unwrap().effective_plan(),
        plan.0
    );
}

pub(super) async fn read_subscription_of(
    world: &mut ApiWorld,
    user_id: universal_inbox::user::UserId,
) -> Option<UserSubscription> {
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

pub(super) async fn create_the_other_user(world: &mut ApiWorld) -> universal_inbox::user::UserId {
    let app = world.app.get().await;
    let user = create_user(
        app,
        format!("{}@billing.test", Uuid::new_v4()).parse().unwrap(),
        "correct horse battery staple",
    )
    .await;
    world.billing.other_user_id = Some(user.id);
    user.id
}

pub(super) fn the_other_user_id(world: &ApiWorld) -> universal_inbox::user::UserId {
    world
        .billing
        .other_user_id
        .expect("No other billing user exists")
}
