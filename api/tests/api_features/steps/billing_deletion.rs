use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

use universal_inbox::billing::SubscriptionStatus;
use universal_inbox_api::repository::user::UserRepository;

use crate::{
    steps::billing_support::{billing, read_subscription, user_id},
    world::ApiWorld,
};

fn stripe_subscription(id: &str, customer: &str, status: &str) -> Value {
    json!({
        "id": id,
        "object": "subscription",
        "automatic_tax": { "enabled": false },
        "billing_cycle_anchor": 1_700_000_000,
        "billing_mode": { "type": "classic" },
        "cancel_at_period_end": false,
        "collection_method": "charge_automatically",
        "created": 1_700_000_000,
        "currency": "eur",
        "customer": customer,
        "discounts": [],
        "invoice_settings": { "issuer": { "type": "self" } },
        "livemode": false,
        "metadata": {},
        "start_date": 1_700_000_000,
        "status": status,
        "items": {
            "object": "list",
            "has_more": false,
            "url": format!("/v1/subscription_items?subscription={id}"),
            "data": [{
                "id": "si_test",
                "object": "subscription_item",
                "created": 1_700_000_000,
                "current_period_start": 1_700_000_000,
                "current_period_end": 1_701_000_000,
                "discounts": [],
                "metadata": {},
                "subscription": id,
                "plan": {
                    "id": "plan_test", "object": "plan", "active": true, "amount": 900,
                    "billing_scheme": "per_unit", "created": 1_700_000_000, "currency": "eur",
                    "interval": "month", "interval_count": 1, "livemode": false,
                    "usage_type": "licensed"
                },
                "price": {
                    "id": "price_test", "object": "price", "active": true,
                    "billing_scheme": "per_unit", "created": 1_700_000_000, "currency": "eur",
                    "livemode": false, "metadata": {}, "product": "prod_test",
                    "type": "recurring"
                }
            }]
        }
    })
}

fn subscription_list(data: Vec<Value>) -> Value {
    json!({ "object": "list", "data": data, "has_more": false, "url": "/v1/subscriptions" })
}

fn mock(world: &ApiWorld) -> &MockServer {
    world
        .billing
        .stripe_mock
        .as_ref()
        .expect("Stripe is not a mock server in this scenario")
}

#[given(
    expr = "Stripe answers as a mock server, which lists no subscription of the customer {string}"
)]
async fn stripe_is_a_mock_server(world: &mut ApiWorld, customer: String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/subscriptions"))
        .and(query_param("customer", customer))
        .respond_with(ResponseTemplate::new(200).set_body_json(subscription_list(vec![])))
        .mount(&server)
        .await;
    world.billing.stripe_mock = Some(server);
}

#[given(
    expr = "Stripe answers as a mock server, which lists the active subscription {string} of the customer {string}"
)]
async fn stripe_lists_a_subscription(world: &mut ApiWorld, subscription: String, customer: String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/subscriptions"))
        .and(query_param("customer", customer.clone()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(subscription_list(vec![stripe_subscription(
                &subscription,
                &customer,
                "active",
            )])),
        )
        .mount(&server)
        .await;
    world.billing.stripe_mock = Some(server);
}

#[given(expr = "Stripe cancels the subscription {string} of the customer {string}, once")]
async fn stripe_cancels_once(world: &mut ApiWorld, subscription: String, customer: String) {
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/subscriptions/{subscription}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(stripe_subscription(
                &subscription,
                &customer,
                "canceled",
            )),
        )
        .expect(1)
        .mount(mock(world))
        .await;
}

#[given(
    expr = "Stripe fails to cancel the subscription {string} of the customer {string}, once, and still shows it as active"
)]
async fn stripe_fails_to_cancel(world: &mut ApiWorld, subscription: String, customer: String) {
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/subscriptions/{subscription}")))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": { "type": "api_error", "message": "Stripe is down" }
        })))
        .expect(1)
        .mount(mock(world))
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/subscriptions/{subscription}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(stripe_subscription(
                &subscription,
                &customer,
                "active",
            )),
        )
        .mount(mock(world))
        .await;
}

#[given(
    expr = "Stripe refuses to cancel the subscription {string} of the customer {string}, as it is already canceled"
)]
async fn stripe_refuses_to_cancel(world: &mut ApiWorld, subscription: String, customer: String) {
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/subscriptions/{subscription}")))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": { "type": "invalid_request_error", "message": "already canceled" }
        })))
        .expect(1)
        .mount(mock(world))
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/subscriptions/{subscription}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(stripe_subscription(
                &subscription,
                &customer,
                "canceled",
            )),
        )
        .mount(mock(world))
        .await;
}

async fn mount_customer_update(world: &ApiWorld, customer: &str, status: u16, expected_calls: u64) {
    Mock::given(method("POST"))
        .and(path(format!("/v1/customers/{customer}")))
        .respond_with(
            ResponseTemplate::new(status)
                .set_body_json(json!({ "id": customer, "object": "customer" })),
        )
        .expect(expected_calls)
        .mount(mock(world))
        .await;
}

#[given(expr = "Stripe accepts to unlink the customer {string} from the user")]
async fn stripe_accepts_to_unlink(world: &mut ApiWorld, customer: String) {
    mount_customer_update(world, &customer, 200, 1).await;
}

#[given(expr = "Stripe fails to unlink the customer {string} from the user")]
async fn stripe_fails_to_unlink(world: &mut ApiWorld, customer: String) {
    mount_customer_update(world, &customer, 500, 1).await;
}

#[when("the user deletes their account")]
async fn the_user_deletes_their_account(world: &mut ApiWorld) {
    let user_id = user_id(world);
    let outcome = {
        let (billing, app) = billing(world).await;
        let mut transaction = app.repository.begin().await.unwrap();
        let result = app
            .user_service
            .delete_user(&mut transaction, user_id, Some(&billing))
            .await;
        if result.is_ok() {
            transaction.commit().await.unwrap();
        }
        result.map_err(|err| format!("{err:?}"))
    };
    world.billing.deletion = Some(outcome);
}

#[then("the account is deleted")]
async fn the_account_is_deleted(world: &mut ApiWorld) {
    assert_eq!(world.billing.deletion, Some(Ok(true)));
    assert!(!user_exists(world).await);
}

#[then("the deletion fails, and the account still exists")]
async fn the_deletion_fails(world: &mut ApiWorld) {
    assert!(matches!(world.billing.deletion, Some(Err(_))));
    assert!(user_exists(world).await);
}

async fn user_exists(world: &mut ApiWorld) -> bool {
    let user_id = user_id(world);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let user = app
        .repository
        .get_user(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    user.is_some()
}

#[then("the subscription of the user is gone")]
async fn the_subscription_is_gone(world: &mut ApiWorld) {
    assert!(read_subscription(world).await.is_none());
}

#[then("the subscription of the user is still tracked as active, so that it cannot be orphaned")]
async fn the_subscription_is_still_tracked(world: &mut ApiWorld) {
    let subscription = read_subscription(world).await.expect("No subscription row");
    assert_eq!(subscription.status, SubscriptionStatus::Active);
}

#[then(expr = "Stripe was told to clear the user of the customer {string}")]
async fn stripe_was_told_to_clear_the_customer(world: &mut ApiWorld, customer: String) {
    let requests = mock(world).received_requests().await.unwrap();
    let update = requests
        .iter()
        .find(|request| request.url.path() == format!("/v1/customers/{customer}"))
        .expect("the customer should be updated");
    let pairs: Vec<(String, String)> = serde_urlencoded::from_bytes(&update.body).unwrap();
    assert!(
        pairs
            .iter()
            .any(|(key, value)| key == "metadata[user_id]" && value.is_empty()),
        "metadata.user_id should be cleared, body was: {pairs:?}"
    );
}

#[then("Stripe received every cancellation that it was told to expect, once")]
async fn stripe_received_the_cancellations(world: &mut ApiWorld) {
    mock(world).verify().await;
}

#[then(
    expr = "Stripe received no request about the customer {string} or the subscription {string}"
)]
async fn stripe_heard_nothing_about(world: &mut ApiWorld, customer: String, subscription: String) {
    let requests = mock(world).received_requests().await.unwrap();
    for request in requests {
        let body = String::from_utf8_lossy(&request.body).to_string();
        let seen = format!(
            "{} {}?{} {body}",
            request.method,
            request.url.path(),
            request.url.query().unwrap_or_default()
        );
        assert!(
            !seen.contains(&customer) && !seen.contains(&subscription),
            "Stripe heard about someone else: {seen}"
        );
    }
}

#[then("the subscription of the other user is still active")]
async fn the_other_subscription_is_active(world: &mut ApiWorld) {
    let other = crate::steps::billing_support::the_other_user_id(world);
    let subscription = crate::steps::billing_support::read_subscription_of(world, other)
        .await
        .expect("The other user lost their subscription row");
    assert_eq!(subscription.status, SubscriptionStatus::Active);
}
