use std::str::FromStr;

use chrono::Utc;
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::Value;
use wiremock::{Mock, ResponseTemplate, matchers::any};

use universal_inbox::billing::SubscriptionStatus;

use crate::{
    helpers::{
        TEST_STRIPE_WEBHOOK_SECRET,
        billing::{stripe_signature, subscription_event},
    },
    steps::billing_support::StripeStatus,
    world::{ApiWorld, Response},
};

#[derive(Clone, Copy, Debug)]
pub enum Signing {
    Missing,
    WrongSecret,
    Stale,
    Valid,
}

impl Parameter for Signing {
    const REGEX: &'static str = "without a signature|signed with a wrong secret|signed correctly 10 minutes ago|signed correctly";
    const NAME: &'static str = "stripe_signing";
}

impl FromStr for Signing {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "without a signature" => Ok(Self::Missing),
            "signed with a wrong secret" => Ok(Self::WrongSecret),
            "signed correctly 10 minutes ago" => Ok(Self::Stale),
            "signed correctly" => Ok(Self::Valid),
            other => Err(format!("Unknown signing {other:?}")),
        }
    }
}

fn stripe_text(status: SubscriptionStatus) -> &'static str {
    match status {
        SubscriptionStatus::Active => "active",
        SubscriptionStatus::PastDue => "past_due",
        SubscriptionStatus::Canceled => "canceled",
        SubscriptionStatus::Incomplete => "incomplete",
        other => panic!("No Stripe text for {other:?}"),
    }
}

#[given("billing with Stripe is enabled")]
fn billing_with_stripe_is_enabled(world: &mut ApiWorld) {
    world.app.configure().billing = true;
}

#[given("my subscription is the one that the events of Stripe are about")]
fn my_subscription_is_the_billing_one(world: &mut ApiWorld) {
    world.billing.user_id = Some(world.user().id);
}

#[given("Stripe is not asked anything")]
async fn stripe_is_not_asked_anything(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(
            app.stripe_mock_server
                .as_ref()
                .expect("Billing is not enabled"),
        )
        .await;
}

#[when(
    expr = "the webhook of Stripe receives, {stripe_signing}, the event {string} telling that the subscription {string} of the customer {string} was updated to {stripe_status}"
)]
async fn the_webhook_receives_an_update(
    world: &mut ApiWorld,
    signing: Signing,
    event_id: String,
    subscription: String,
    customer: String,
    status: StripeStatus,
) {
    let now = Utc::now().timestamp();
    let payload = subscription_event(
        &event_id,
        "customer.subscription.updated",
        &customer,
        &subscription,
        stripe_text(status.0),
        now,
    );
    let signature = match signing {
        Signing::Missing => None,
        Signing::WrongSecret => Some(stripe_signature("whsec_not_the_secret", now, &payload)),
        Signing::Stale => Some(stripe_signature(
            TEST_STRIPE_WEBHOOK_SECRET,
            now - 600,
            &payload,
        )),
        Signing::Valid => Some(stripe_signature(TEST_STRIPE_WEBHOOK_SECRET, now, &payload)),
    };
    let app = world.app.get().await;
    let mut request = reqwest::Client::new()
        .post(format!("{}billing/stripe/webhook", app.api_address))
        .header("content-type", "application/json")
        .body(payload);
    if let Some(signature) = signature {
        request = request.header("Stripe-Signature", signature);
    }
    let response = request.send().await.expect("Failed to call the webhook");
    world.http.record(Response::read(response).await);
}

#[when("I read my billing state")]
async fn i_read_my_billing_state(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .get(format!("{}billing/me", app.api_address))
        .send()
        .await
        .expect("Failed to read the billing state");
    world.http.record(Response::read(response).await);
}

#[then(expr = "the answer is the refusal {string}")]
fn the_answer_is_the_refusal(world: &mut ApiWorld, message: String) {
    let body: Value = world.http.last().json();
    assert_eq!(body, serde_json::json!({ "message": message }));
}

#[then(expr = "my billing state says the {word} plan, with the subscription status {word}")]
fn my_billing_state_says(world: &mut ApiWorld, plan: String, status: String) {
    assert_eq!(world.http.last().status, 200);
    let body: Value = world.http.last().json();
    assert_eq!(
        (body["plan"].clone(), body["status"].clone()),
        (Value::String(plan), Value::String(status))
    );
}
