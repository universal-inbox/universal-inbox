use std::sync::Arc;

use chrono::{DateTime, Utc};
use rstest::*;
use serde_json::json;
use universal_inbox::billing::SubscriptionStatus as DomainStatus;
use universal_inbox::user::UserId;
use universal_inbox_api::billing::repository::BillingRepository;
use universal_inbox_api::billing::service::BillingService;
use universal_inbox_api::billing::stripe::{RawSubscription, StripeEvent, StripeEventKind};

use crate::helpers::{TestedApp, tested_app_with_local_auth};

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
        async fn cancel_subscription(
            &self,
            _subscription_id: &str,
        ) -> Result<RawSubscription, StripeError> {
            Err(StripeError::Other(anyhow::anyhow!(
                "cancel_subscription is not exercised by these tests"
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

/// Account deletion must stop Stripe billing *before* the local rows go away:
/// a deleted user must never keep being charged.
mod account_deletion {
    use super::*;

    use secrecy::SecretBox;
    use universal_inbox_api::{
        billing::{service::RepositoryIntegrationCounter, stripe::client::StripeApiClient},
        configuration::{FreePlanSettings, StripeApiKey, StripeWebhookSecret},
        repository::user::UserRepository,
    };
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    /// A `BillingService` backed by the real async-stripe client, pointed at a
    /// wiremock server: the test asserts on the actual Stripe HTTP requests.
    fn billing_service_against(app: &TestedApp, stripe_mock: &MockServer) -> Arc<BillingService> {
        let stripe = Arc::new(StripeApiClient::new_with_base_url(
            &SecretBox::new(Box::new(StripeApiKey("dummy-stripe-api-key".to_string()))),
            SecretBox::new(Box::new(StripeWebhookSecret(
                "dummy-webhook-secret".to_string(),
            ))),
            &stripe_mock.uri(),
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

    /// A Stripe subscription object in the current API shape (the billing
    /// period lives under `items.data[]`).
    fn stripe_subscription(id: &str, customer: &str, status: &str) -> serde_json::Value {
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

    fn subscription_list(data: Vec<serde_json::Value>) -> serde_json::Value {
        json!({ "object": "list", "data": data, "has_more": false, "url": "/v1/subscriptions" })
    }

    async fn paid_user(app: &TestedApp, email: &str, customer: &str, subscription: &str) -> UserId {
        let user = crate::helpers::user::create_user(
            app,
            email.parse().unwrap(),
            "correct horse battery staple",
        )
        .await;
        let seeding = build_test_billing_service(app);
        apply(
            app,
            &seeding,
            &checkout_event(ts(1_700_000_000), customer, subscription, user.id),
            Some(raw_sub(subscription, customer, DomainStatus::Active)),
        )
        .await;
        user.id
    }

    async fn delete_user(
        app: &TestedApp,
        billing: &Arc<BillingService>,
        user_id: UserId,
    ) -> Result<bool, universal_inbox_api::universal_inbox::UniversalInboxError> {
        let mut transaction = app.repository.begin().await.unwrap();
        let result = app
            .user_service
            .delete_user(&mut transaction, user_id, Some(billing))
            .await;
        if result.is_ok() {
            transaction.commit().await.unwrap();
        }
        result
    }

    async fn user_exists(app: &TestedApp, user_id: UserId) -> bool {
        let mut transaction = app.repository.begin().await.unwrap();
        let user = app
            .repository
            .get_user(&mut transaction, user_id)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        user.is_some()
    }

    async fn mount_customer_update(stripe_mock: &MockServer, customer: &str, status: u16) {
        Mock::given(method("POST"))
            .and(path(format!("/v1/customers/{customer}")))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(json!({ "id": customer, "object": "customer" })),
            )
            .mount(stripe_mock)
            .await;
    }

    #[rstest]
    #[tokio::test]
    async fn deleting_a_paid_user_cancels_the_stripe_subscription(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;
        let user_id = paid_user(&app, "del1@billing.test", "cus_del1", "sub_del1").await;

        let stripe_mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/subscriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(subscription_list(vec![])))
            .mount(&stripe_mock)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/v1/subscriptions/sub_del1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(stripe_subscription("sub_del1", "cus_del1", "canceled")),
            )
            .expect(1)
            .mount(&stripe_mock)
            .await;
        mount_customer_update(&stripe_mock, "cus_del1", 200).await;

        let billing = billing_service_against(&app, &stripe_mock);
        assert!(
            delete_user(&app, &billing, user_id)
                .await
                .expect("deleting a paid user should succeed")
        );

        assert!(!user_exists(&app, user_id).await);
        assert!(read_subscription(&app, &billing, user_id).await.is_none());

        // The customer is kept (invoices) but unlinked from the deleted user.
        let requests = stripe_mock.received_requests().await.unwrap();
        let customer_update = requests
            .iter()
            .find(|request| request.url.path() == "/v1/customers/cus_del1")
            .expect("the customer should be updated");
        let pairs: Vec<(String, String)> =
            serde_urlencoded::from_bytes(&customer_update.body).unwrap();
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "metadata[user_id]" && value.is_empty()),
            "metadata.user_id should be cleared, body was: {pairs:?}"
        );
        stripe_mock.verify().await;
    }

    #[rstest]
    #[tokio::test]
    async fn deleting_a_user_also_cancels_subscriptions_only_stripe_knows(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;
        let user_id = paid_user(&app, "del2@billing.test", "cus_del2", "sub_del2").await;

        let stripe_mock = MockServer::start().await;
        // Stripe also holds a second, still active, subscription whose webhook
        // never reached us.
        Mock::given(method("GET"))
            .and(path("/v1/subscriptions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(subscription_list(vec![
                    stripe_subscription("sub_del2_missed", "cus_del2", "active"),
                ])),
            )
            .mount(&stripe_mock)
            .await;
        for subscription in ["sub_del2", "sub_del2_missed"] {
            Mock::given(method("DELETE"))
                .and(path(format!("/v1/subscriptions/{subscription}")))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(stripe_subscription(
                        subscription,
                        "cus_del2",
                        "canceled",
                    )),
                )
                .expect(1)
                .mount(&stripe_mock)
                .await;
        }
        mount_customer_update(&stripe_mock, "cus_del2", 200).await;

        let billing = billing_service_against(&app, &stripe_mock);
        delete_user(&app, &billing, user_id)
            .await
            .expect("deleting a paid user should succeed");

        assert!(!user_exists(&app, user_id).await);
        stripe_mock.verify().await;
    }

    #[rstest]
    #[tokio::test]
    async fn a_failed_stripe_cancellation_aborts_the_account_deletion(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;
        let user_id = paid_user(&app, "del3@billing.test", "cus_del3", "sub_del3").await;

        let stripe_mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/subscriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(subscription_list(vec![])))
            .mount(&stripe_mock)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/v1/subscriptions/sub_del3"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "error": { "type": "api_error", "message": "Stripe is down" }
            })))
            .expect(1)
            .mount(&stripe_mock)
            .await;
        // Re-reading the subscription still shows it active on Stripe's side.
        Mock::given(method("GET"))
            .and(path("/v1/subscriptions/sub_del3"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(stripe_subscription("sub_del3", "cus_del3", "active")),
            )
            .mount(&stripe_mock)
            .await;

        let billing = billing_service_against(&app, &stripe_mock);
        assert!(
            delete_user(&app, &billing, user_id).await.is_err(),
            "the deletion must fail when the subscription could not be cancelled"
        );

        // Nothing was deleted: the user can retry, and the subscription is
        // still tracked so it can't be orphaned.
        assert!(user_exists(&app, user_id).await);
        let subscription = read_subscription(&app, &billing, user_id).await.unwrap();
        assert_eq!(subscription.status, DomainStatus::Active);
        stripe_mock.verify().await;
    }

    #[rstest]
    #[tokio::test]
    async fn an_already_canceled_subscription_does_not_block_the_account_deletion(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;
        let user_id = paid_user(&app, "del4@billing.test", "cus_del4", "sub_del4").await;

        let stripe_mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/subscriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(subscription_list(vec![])))
            .mount(&stripe_mock)
            .await;
        // Cancelled from the portal, webhook not received yet: Stripe rejects
        // the cancel, and a re-read shows it is already canceled.
        Mock::given(method("DELETE"))
            .and(path("/v1/subscriptions/sub_del4"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": { "type": "invalid_request_error", "message": "already canceled" }
            })))
            .mount(&stripe_mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/subscriptions/sub_del4"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(stripe_subscription("sub_del4", "cus_del4", "canceled")),
            )
            .mount(&stripe_mock)
            .await;
        // Unlinking the customer metadata fails: logged, not fatal.
        mount_customer_update(&stripe_mock, "cus_del4", 500).await;

        let billing = billing_service_against(&app, &stripe_mock);
        delete_user(&app, &billing, user_id)
            .await
            .expect("an already canceled subscription must not block the deletion");
        assert!(!user_exists(&app, user_id).await);
    }
}
