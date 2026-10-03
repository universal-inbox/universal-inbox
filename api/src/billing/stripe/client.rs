//! Thin wrapper around `async-stripe`. Public methods accept and return
//! domain types (or the local raw structs defined here) so Stripe SDK types
//! never escape into [`crate::billing::service`].
//!
//! For testability, the API surface is exposed as a trait
//! ([`StripeClient`]) plus an `async-stripe`-backed implementation
//! ([`StripeApiClient`]). Tests can stub the trait directly without touching
//! HTTP.

use std::future::Future;

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use secrecy::{ExposeSecret, SecretBox};
use stripe::{Client as AsyncStripeClient, StripeError as SdkError};
use stripe_billing::billing_portal_session::CreateBillingPortalSession;
use stripe_billing::subscription::{
    CancelSubscription, ListSubscription, ListSubscriptionStatus, RetrieveSubscription,
};
use stripe_checkout::checkout_session::{
    CreateCheckoutSession, CreateCheckoutSessionAutomaticTax, CreateCheckoutSessionCustomerUpdate,
    CreateCheckoutSessionCustomerUpdateAddress, CreateCheckoutSessionLineItems,
};
use stripe_core::customer::{CreateCustomer, UpdateCustomer};
use stripe_shared::{ApiErrorsType, CheckoutSessionMode, Subscription, SubscriptionStatus};
use stripe_webhook::{Event, EventObject, Webhook, WebhookError};
use thiserror::Error;
use universal_inbox::{
    billing::{StripeCustomerId, StripePriceId, SubscriptionStatus as DomainStatus},
    user::UserId,
};
use url::Url;

use crate::{
    configuration::{StripeApiKey, StripeWebhookSecret},
    observability::{http_client_span, instrument_client_call, spans::OTHER_ERROR_TYPE},
    universal_inbox::{UniversalInboxError, UpstreamErrorKind},
};

/// Errors produced by the Stripe adapter.
#[derive(Debug, Error)]
pub enum StripeError {
    #[error("Stripe API error: {0}")]
    Api(#[from] stripe::StripeError),
    #[error("Stripe webhook signature is invalid")]
    InvalidWebhookSignature,
    #[error("Stripe webhook timestamp is outside the acceptable tolerance")]
    StaleWebhookTimestamp,
    #[error("Stripe webhook payload is malformed: {0}")]
    MalformedWebhookPayload(String),
    #[error("Stripe returned no Checkout URL")]
    MissingCheckoutUrl,
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Map Stripe adapter errors to `UniversalInboxError` with a faithful HTTP
/// status instead of collapsing every failure to an opaque 500. Lives in the
/// billing module (not the core error type) so deleting the billing tree
/// removes this mapping cleanly. Distinguishes card errors (402), rate limits
/// (429), and invalid requests / bad price id (400) from genuine server-side
/// faults (500).
/// Map a Stripe API HTTP status (and whether it was a card error) to a faithful
/// `UpstreamErrorKind`. Pulled out as a pure function so it can be unit-tested
/// without constructing a full `stripe::StripeError` (whose `ApiErrors` payload
/// has many fields). async-stripe 1.0's `ApiErrorsType` no longer distinguishes
/// rate-limit or auth errors, so the HTTP status is the primary signal.
fn upstream_kind_for_stripe_status(
    status: u16,
    is_card_error: bool,
) -> (UpstreamErrorKind, &'static str) {
    if is_card_error || status == 402 {
        (UpstreamErrorKind::PaymentRequired, "stripe_card_error")
    } else {
        match status {
            429 => (UpstreamErrorKind::RateLimited, "stripe_rate_limited"),
            // A request we built was rejected (e.g. a bad price id) — 400.
            400 | 404 | 409 | 422 => (UpstreamErrorKind::BadRequest, "stripe_invalid_request"),
            // Auth (401/403) and server faults (5xx) are our config or the
            // provider's problem, not the caller's — surface as 500.
            _ => (UpstreamErrorKind::Internal, "stripe_api_error"),
        }
    }
}

impl From<StripeError> for UniversalInboxError {
    fn from(err: StripeError) -> Self {
        let (kind, code) = match &err {
            StripeError::InvalidWebhookSignature
            | StripeError::StaleWebhookTimestamp
            | StripeError::MalformedWebhookPayload(_) => {
                (UpstreamErrorKind::BadRequest, "stripe_invalid_webhook")
            }
            StripeError::MissingCheckoutUrl => {
                (UpstreamErrorKind::Internal, "stripe_missing_checkout_url")
            }
            StripeError::Other(_) => (UpstreamErrorKind::Internal, "stripe_error"),
            // 1.0 carries the HTTP status alongside the parsed error body.
            StripeError::Api(SdkError::Stripe(api_errors, status)) => {
                upstream_kind_for_stripe_status(
                    *status,
                    matches!(api_errors.type_, ApiErrorsType::CardError),
                )
            }
            // Serialization / transport / config / timeout faults are ours or
            // the network's, not the caller's.
            StripeError::Api(_) => (UpstreamErrorKind::Internal, "stripe_error"),
        };

        UniversalInboxError::UpstreamServiceError {
            kind,
            code,
            message: format!("Stripe request failed: {err}"),
        }
    }
}

/// Run a Stripe API call inside an INFO client span: async-stripe uses its own
/// hyper client, which the reqwest tracing middleware does not cover.
async fn stripe_api_call<T>(
    method: &'static str,
    route: &str,
    call: impl Future<Output = Result<T, SdkError>>,
) -> Result<T, SdkError> {
    instrument_client_call(
        http_client_span(method, "api.stripe.com", route),
        call,
        |error| match error {
            SdkError::Stripe(_, status) => status.to_string(),
            _ => OTHER_ERROR_TYPE.to_string(),
        },
    )
    .await
}

/// Subset of fields we persist from a Stripe Subscription. Defined here so
/// `stripe::Subscription` never leaks into the service layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSubscription {
    pub subscription_id: String,
    pub customer_id: String,
    pub price_id: Option<String>,
    pub status: DomainStatus,
    pub current_period_start: Option<DateTime<Utc>>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub canceled_at: Option<DateTime<Utc>>,
}

/// Subset of Stripe webhook event kinds that drive billing state. Other event
/// types from Stripe are mapped to [`StripeEventKind::Other`] and ignored by
/// the webhook handler (still recorded for idempotency).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StripeEventKind {
    CheckoutSessionCompleted {
        customer_id: Option<String>,
        subscription_id: Option<String>,
        user_id_metadata: Option<UserId>,
    },
    SubscriptionUpserted(RawSubscription),
    SubscriptionDeleted(RawSubscription),
    InvoicePaid {
        customer_id: Option<String>,
        subscription_id: Option<String>,
    },
    InvoicePaymentFailed {
        customer_id: Option<String>,
        subscription_id: Option<String>,
    },
    Other,
}

/// A verified-and-parsed webhook event. The event id is exposed so the
/// handler can record it in the idempotency table before processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeEvent {
    pub id: String,
    pub type_: String,
    /// Stripe's `event.created` (the moment Stripe generated the event).
    /// Used as an ordering guard for out-of-order subscription webhooks —
    /// Stripe does not guarantee delivery order.
    pub created: DateTime<Utc>,
    pub kind: StripeEventKind,
}

#[derive(Debug, Clone)]
pub struct CheckoutSessionParams {
    pub customer_id: StripeCustomerId,
    pub price_id: StripePriceId,
    pub success_url: Url,
    pub cancel_url: Url,
    pub user_id: UserId,
}

#[derive(Debug, Clone)]
pub struct PortalSessionParams {
    pub customer_id: StripeCustomerId,
    pub return_url: Url,
}

#[async_trait]
pub trait StripeClient: Send + Sync {
    /// Create a new Stripe Customer for a Universal Inbox user, returning the
    /// freshly-minted Stripe customer id. The user id is stamped onto
    /// `metadata.user_id` so the webhook handler can resolve it without
    /// trusting external URLs.
    async fn create_customer(
        &self,
        user_id: UserId,
        email: Option<&str>,
    ) -> Result<String, StripeError>;

    /// Create a Checkout session bound to the given customer. Returns the
    /// hosted URL the UI should redirect to. `automatic_tax` is unconditionally
    /// enabled so Stripe Tax handles EU VAT.
    async fn create_checkout_session(
        &self,
        params: CheckoutSessionParams,
    ) -> Result<Url, StripeError>;

    /// Create a Billing Portal session. Returns the hosted URL.
    async fn create_portal_session(&self, params: PortalSessionParams) -> Result<Url, StripeError>;

    /// Fetch the current state of a subscription, mapped to [`RawSubscription`].
    async fn fetch_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<RawSubscription, StripeError>;

    /// List the subscriptions Stripe holds for a customer, newest first and
    /// including terminal ones (`status=all`). The reconcile job uses this to
    /// adopt a subscription whose `checkout.session.completed` webhook never
    /// arrived: the local row then knows only the customer id, so there is no
    /// subscription id to retrieve.
    async fn list_customer_subscriptions(
        &self,
        customer_id: &str,
    ) -> Result<Vec<RawSubscription>, StripeError>;

    /// Cancel a subscription immediately (not at period end), so no further
    /// invoice is issued. Used when the user deletes their account. Past
    /// invoices are kept by Stripe.
    async fn cancel_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<RawSubscription, StripeError>;

    /// Remove the `metadata.user_id` link from a Stripe Customer, so the
    /// customer (kept for invoices / accounting retention) no longer points to
    /// a deleted Universal Inbox user.
    async fn clear_customer_user_id(&self, customer_id: &str) -> Result<(), StripeError>;

    /// Verify the signature on an inbound webhook delivery and parse the
    /// event into a [`StripeEvent`].
    fn verify_webhook_signature(
        &self,
        payload: &str,
        signature_header: &str,
    ) -> Result<StripeEvent, StripeError>;
}

/// How many subscriptions a single customer listing pulls. A Universal Inbox
/// customer has one subscription in the normal case; the margin only covers a
/// history of re-subscriptions, and adoption reads the newest match anyway.
const CUSTOMER_SUBSCRIPTIONS_PAGE_SIZE: i64 = 20;

/// Production implementation backed by `async-stripe`.
pub struct StripeApiClient {
    inner: AsyncStripeClient,
    webhook_secret: SecretBox<StripeWebhookSecret>,
}

/// Ensure a process-wide rustls [`CryptoProvider`] is installed before
/// async-stripe builds its hyper-rustls connector. The workspace links **both**
/// the `ring` and `aws-lc-rs` rustls providers (two reqwest versions pull
/// different ones), so rustls cannot auto-select a default and
/// `ClientConfig::builder()` panics. reqwest configures its own provider
/// per-client and so is unaffected, but async-stripe relies on the process
/// default — so we install one explicitly (idempotently) here.
fn ensure_crypto_provider() {
    use rustls::crypto::{CryptoProvider, aws_lc_rs};

    if CryptoProvider::get_default().is_none() {
        let _ = aws_lc_rs::default_provider().install_default();
    }
}

impl StripeApiClient {
    pub fn new(
        secret_key: &SecretBox<StripeApiKey>,
        webhook_secret: SecretBox<StripeWebhookSecret>,
    ) -> Self {
        ensure_crypto_provider();
        Self {
            inner: AsyncStripeClient::new(secret_key.expose_secret().0.clone()),
            webhook_secret,
        }
    }

    /// Build a client whose HTTP calls hit `base_url` instead of the live
    /// Stripe API — used by tests to point the *real* request-construction
    /// path at a wiremock instance and assert on the outgoing request body.
    /// async-stripe 1.0's `ClientBuilder::url` overrides the API base.
    /// Not gated on `cfg(test)` so the API integration tests (a separate
    /// crate) can use it too.
    pub fn new_with_base_url(
        secret_key: &SecretBox<StripeApiKey>,
        webhook_secret: SecretBox<StripeWebhookSecret>,
        base_url: &str,
    ) -> Self {
        ensure_crypto_provider();
        Self {
            inner: stripe::ClientBuilder::new(secret_key.expose_secret().0.clone())
                .url(base_url)
                // `RequestStrategy::Once` disables retries so a single mocked
                // response is enough and the test doesn't hang retrying.
                .request_strategy(stripe::RequestStrategy::Once)
                .build()
                .expect("valid Stripe client configuration"),
            webhook_secret,
        }
    }
}

#[async_trait]
impl StripeClient for StripeApiClient {
    async fn create_customer(
        &self,
        user_id: UserId,
        email: Option<&str>,
    ) -> Result<String, StripeError> {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("user_id".to_string(), user_id.0.to_string());

        let mut request = CreateCustomer::new().metadata(metadata);
        if let Some(email) = email {
            request = request.email(email.to_string());
        }

        let customer = stripe_api_call("POST", "/v1/customers", request.send(&self.inner)).await?;
        Ok(customer.id.to_string())
    }

    async fn create_checkout_session(
        &self,
        params: CheckoutSessionParams,
    ) -> Result<Url, StripeError> {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("user_id".to_string(), params.user_id.0.to_string());

        let mut line_item = CreateCheckoutSessionLineItems::new();
        line_item.price = Some(params.price_id.0.clone());
        line_item.quantity = Some(1);
        let line_items = vec![line_item];

        // `automatic_tax` needs an address on the Customer to compute tax. We
        // pass a pre-created `customer`, so Stripe rejects the session unless we
        // explicitly let Checkout save the billing address it collects back onto
        // the customer (`customer_update[address] = auto`). Without this Stripe
        // returns invalid_request_error ("Automatic tax calculation in Checkout
        // requires a valid address on the Customer") and the upgrade click fails.
        let mut customer_update = CreateCheckoutSessionCustomerUpdate::new();
        customer_update.address = Some(CreateCheckoutSessionCustomerUpdateAddress::Auto);

        let session = CreateCheckoutSession::new()
            .mode(CheckoutSessionMode::Subscription)
            .customer(params.customer_id.0.clone())
            .success_url(params.success_url.as_str())
            .cancel_url(params.cancel_url.as_str())
            .line_items(line_items)
            .automatic_tax(CreateCheckoutSessionAutomaticTax::new(true))
            .customer_update(customer_update)
            .metadata(metadata);
        let session =
            stripe_api_call("POST", "/v1/checkout/sessions", session.send(&self.inner)).await?;

        let url = session.url.ok_or(StripeError::MissingCheckoutUrl)?;
        Url::parse(&url)
            .map_err(|err| StripeError::Other(anyhow!("Stripe returned an unparseable URL: {err}")))
    }

    async fn create_portal_session(&self, params: PortalSessionParams) -> Result<Url, StripeError> {
        let session = CreateBillingPortalSession::new()
            .customer(params.customer_id.0.clone())
            .return_url(params.return_url.as_str());
        let session = stripe_api_call(
            "POST",
            "/v1/billing_portal/sessions",
            session.send(&self.inner),
        )
        .await?;
        Url::parse(&session.url)
            .map_err(|err| StripeError::Other(anyhow!("Stripe returned an unparseable URL: {err}")))
    }

    async fn fetch_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<RawSubscription, StripeError> {
        let sub = stripe_api_call(
            "GET",
            "/v1/subscriptions/{subscription_id}",
            RetrieveSubscription::new(subscription_id).send(&self.inner),
        )
        .await?;
        Ok(RawSubscription::from(&sub))
    }

    async fn list_customer_subscriptions(
        &self,
        customer_id: &str,
    ) -> Result<Vec<RawSubscription>, StripeError> {
        let list = ListSubscription::new()
            .customer(customer_id)
            .status(ListSubscriptionStatus::All)
            .limit(CUSTOMER_SUBSCRIPTIONS_PAGE_SIZE);
        let list = stripe_api_call("GET", "/v1/subscriptions", list.send(&self.inner)).await?;
        Ok(list.data.iter().map(RawSubscription::from).collect())
    }

    async fn cancel_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<RawSubscription, StripeError> {
        let sub = stripe_api_call(
            "DELETE",
            "/v1/subscriptions/{subscription_id}",
            CancelSubscription::new(subscription_id).send(&self.inner),
        )
        .await?;
        Ok(RawSubscription::from(&sub))
    }

    async fn clear_customer_user_id(&self, customer_id: &str) -> Result<(), StripeError> {
        // Setting a metadata key to an empty string removes it on Stripe's side.
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("user_id".to_string(), String::new());
        let request = UpdateCustomer::new(customer_id).metadata(metadata);
        stripe_api_call(
            "POST",
            "/v1/customers/{customer_id}",
            request.send(&self.inner),
        )
        .await?;
        Ok(())
    }

    fn verify_webhook_signature(
        &self,
        payload: &str,
        signature_header: &str,
    ) -> Result<StripeEvent, StripeError> {
        let secret = &self.webhook_secret.expose_secret().0;
        match Webhook::construct_event(payload, signature_header, secret) {
            Ok(event) => StripeEvent::try_from(event),
            Err(WebhookError::BadSignature | WebhookError::BadKey | WebhookError::BadHeader(_)) => {
                Err(StripeError::InvalidWebhookSignature)
            }
            Err(WebhookError::BadTimestamp(_)) => Err(StripeError::StaleWebhookTimestamp),
            Err(WebhookError::BadParse(err)) => {
                Err(StripeError::MalformedWebhookPayload(err.to_string()))
            }
        }
    }
}

// Map a verified Stripe webhook `Event` into our local [`StripeEvent`].
//
// Fails (rather than silently defaulting) if Stripe's `event.created`
// timestamp can't be represented — the ordering guard downstream depends on
// a real timestamp, so a bogus one must be rejected loudly.
impl TryFrom<Event> for StripeEvent {
    type Error = StripeError;

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        let created = timestamp_to_datetime(Some(event.created)).ok_or_else(|| {
            StripeError::MalformedWebhookPayload(format!(
                "event {} carries an out-of-range created timestamp ({})",
                event.id, event.created
            ))
        })?;
        // In async-stripe 1.0 each `EventObject` variant already encodes the
        // event type, so we match the object directly rather than on the
        // (type, object) pair.
        let kind = match event.data.object {
            EventObject::CheckoutSessionCompleted(session) => {
                let user_id_metadata = session
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("user_id"))
                    .and_then(|s| s.parse::<uuid::Uuid>().ok())
                    .map(UserId);
                StripeEventKind::CheckoutSessionCompleted {
                    customer_id: session.customer.as_ref().map(|c| c.id().to_string()),
                    subscription_id: session.subscription.as_ref().map(|s| s.id().to_string()),
                    user_id_metadata,
                }
            }
            EventObject::CustomerSubscriptionCreated(sub)
            | EventObject::CustomerSubscriptionUpdated(sub) => {
                StripeEventKind::SubscriptionUpserted(RawSubscription::from(sub.as_ref()))
            }
            EventObject::CustomerSubscriptionDeleted(sub) => {
                StripeEventKind::SubscriptionDeleted(RawSubscription::from(sub.as_ref()))
            }
            EventObject::InvoicePaid(invoice) => StripeEventKind::InvoicePaid {
                customer_id: invoice.customer.as_ref().map(|c| c.id().to_string()),
                subscription_id: invoice.subscription.as_ref().map(|s| s.id().to_string()),
            },
            EventObject::InvoicePaymentFailed(invoice) => StripeEventKind::InvoicePaymentFailed {
                customer_id: invoice.customer.as_ref().map(|c| c.id().to_string()),
                subscription_id: invoice.subscription.as_ref().map(|s| s.id().to_string()),
            },
            _ => StripeEventKind::Other,
        };
        Ok(StripeEvent {
            id: event.id.to_string(),
            type_: event.type_.as_str().to_string(),
            created,
            kind,
        })
    }
}

impl From<&Subscription> for RawSubscription {
    fn from(sub: &Subscription) -> Self {
        // Stripe API 2025-03-31+ moved the billing period off the top-level
        // subscription object onto each subscription item; we read it (and the
        // price) from the first item.
        let first_item = sub.items.data.first();
        let price_id = first_item.map(|item| item.price.id.to_string());
        let current_period_start =
            first_item.and_then(|item| timestamp_to_datetime(Some(item.current_period_start)));
        let current_period_end =
            first_item.and_then(|item| timestamp_to_datetime(Some(item.current_period_end)));

        RawSubscription {
            subscription_id: sub.id.to_string(),
            customer_id: sub.customer.id().to_string(),
            price_id,
            status: map_status(&sub.status),
            current_period_start,
            current_period_end,
            // Stripe schedules a future cancellation two ways: `cancel_at_period_end`
            // (cancel at the end of the current period) or `cancel_at` (cancel at a
            // specific time — what the billing portal now sets). Either means the
            // subscription is winding down, so surface both as a pending
            // cancellation; otherwise a portal-initiated cancel shows as "renewing".
            cancel_at_period_end: sub.cancel_at_period_end || sub.cancel_at.is_some(),
            canceled_at: timestamp_to_datetime(sub.canceled_at),
        }
    }
}

fn map_status(status: &SubscriptionStatus) -> DomainStatus {
    match status {
        SubscriptionStatus::Active => DomainStatus::Active,
        SubscriptionStatus::PastDue => DomainStatus::PastDue,
        SubscriptionStatus::Canceled => DomainStatus::Canceled,
        SubscriptionStatus::Unpaid => DomainStatus::Unpaid,
        SubscriptionStatus::Incomplete => DomainStatus::Incomplete,
        SubscriptionStatus::IncompleteExpired => DomainStatus::IncompleteExpired,
        SubscriptionStatus::Trialing => DomainStatus::Trialing,
        SubscriptionStatus::Paused => DomainStatus::Paused,
        // `SubscriptionStatus` is `#[non_exhaustive]` and carries an
        // `Unknown(_)` variant for values from newer API versions. Treat any
        // such status as non-entitled (effectively Free) without marking it
        // terminal.
        _ => DomainStatus::Incomplete,
    }
}

fn timestamp_to_datetime(ts: Option<i64>) -> Option<DateTime<Utc>> {
    ts.and_then(|secs| Utc.timestamp_opt(secs, 0).single())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretBox;

    fn webhook_secret(secret: &str) -> SecretBox<StripeWebhookSecret> {
        SecretBox::new(Box::new(StripeWebhookSecret(secret.to_string())))
    }

    /// Stripe failures must keep their identity through the error boundary:
    /// card errors → 402, rate limits → 429, bad requests → 400, and
    /// auth/server faults → 500 — instead of all collapsing to an opaque 500.
    /// async-stripe 1.0's `ApiErrorsType` no longer carries rate-limit/auth
    /// variants, so the mapping keys on the HTTP status; we test that pure
    /// helper directly rather than hand-building a full `ApiErrors`.
    #[test]
    fn stripe_status_maps_to_faithful_kind() {
        use UpstreamErrorKind::*;

        // A card decline surfaces as 402 or via the error type.
        assert_eq!(
            upstream_kind_for_stripe_status(402, false).0,
            PaymentRequired
        );
        assert_eq!(
            upstream_kind_for_stripe_status(400, true).0,
            PaymentRequired
        );
        assert_eq!(upstream_kind_for_stripe_status(429, false).0, RateLimited);
        assert_eq!(
            upstream_kind_for_stripe_status(400, false).0,
            BadRequest,
            "a bad price id is a 400, not a 500"
        );
        assert_eq!(
            upstream_kind_for_stripe_status(401, false).0,
            Internal,
            "a bad API key is our fault, not the caller's"
        );
        assert_eq!(upstream_kind_for_stripe_status(500, false).0, Internal);
    }

    /// Non-API adapter errors keep faithful kinds too: a malformed/invalid
    /// webhook is the caller's fault (400), a missing checkout URL is ours (500).
    #[test]
    fn adapter_errors_map_to_faithful_kinds() {
        fn kind_of(err: StripeError) -> UpstreamErrorKind {
            match UniversalInboxError::from(err) {
                UniversalInboxError::UpstreamServiceError { kind, .. } => kind,
                other => panic!("expected UpstreamServiceError, got {other:?}"),
            }
        }

        assert_eq!(
            kind_of(StripeError::InvalidWebhookSignature),
            UpstreamErrorKind::BadRequest
        );
        assert_eq!(
            kind_of(StripeError::MalformedWebhookPayload("boom".into())),
            UpstreamErrorKind::BadRequest
        );
        assert_eq!(
            kind_of(StripeError::MissingCheckoutUrl),
            UpstreamErrorKind::Internal
        );
    }

    fn api_key() -> SecretBox<StripeApiKey> {
        SecretBox::new(Box::new(StripeApiKey("sk_test_dummy".to_string())))
    }

    /// Build a signed-ready `customer.subscription.*` webhook payload in the
    /// **current** Stripe API shape (period under `items.data[]`, not top-level).
    /// `cancel_at` is added to the subscription object when `Some`.
    fn subscription_event_payload(
        event_type: &str,
        status: &str,
        cancel_at_period_end: bool,
        cancel_at: Option<i64>,
    ) -> String {
        let mut object = serde_json::json!({
            "id": "sub_test123",
            "object": "subscription",
            "automatic_tax": { "enabled": false },
            "billing_cycle_anchor": 1_700_000_000,
            "billing_mode": { "type": "classic" },
            "cancel_at_period_end": cancel_at_period_end,
            "collection_method": "charge_automatically",
            "created": 1_700_000_000,
            "currency": "eur",
            "customer": "cus_test123",
            "discounts": [],
            "invoice_settings": { "issuer": { "type": "self" } },
            "livemode": false,
            "metadata": {},
            "start_date": 1_700_000_000,
            "status": status,
            "items": {
                "object": "list",
                "has_more": false,
                "url": "/v1/subscription_items?subscription=sub_test123",
                "data": [{
                    "id": "si_test123",
                    "object": "subscription_item",
                    "created": 1_700_000_000,
                    "current_period_start": 1_700_000_000,
                    "current_period_end": 1_701_000_000,
                    "discounts": [],
                    "metadata": {},
                    "subscription": "sub_test123",
                    "plan": {
                        "id": "plan_test123", "object": "plan", "active": true, "amount": 900,
                        "billing_scheme": "per_unit", "created": 1_700_000_000, "currency": "eur",
                        "interval": "month", "interval_count": 1, "livemode": false,
                        "usage_type": "licensed"
                    },
                    "price": {
                        "id": "price_test123", "object": "price", "active": true,
                        "billing_scheme": "per_unit", "created": 1_700_000_000, "currency": "eur",
                        "livemode": false, "metadata": {}, "product": "prod_test123",
                        "type": "recurring"
                    }
                }]
            }
        });
        if let Some(ts) = cancel_at {
            object["cancel_at"] = serde_json::json!(ts);
            object["canceled_at"] = serde_json::json!(1_700_000_050);
        }
        serde_json::json!({
            "id": "evt_sub",
            "object": "event",
            "api_version": "2025-03-31.basil",
            "created": 1_700_000_000,
            "livemode": false,
            "pending_webhooks": 1,
            "type": event_type,
            "data": { "object": object }
        })
        .to_string()
    }

    /// Regression guard for the subscription-webhook 400s. A
    /// `customer.subscription.deleted` event in the **current** Stripe API shape
    /// (Stripe API 2025-03-31+: the billing period lives under `items.data[]`,
    /// not at the top level) must verify and map to `SubscriptionDeleted` with
    /// the period read from the item. async-stripe 0.41 required the now-removed
    /// top-level `current_period_*` fields, so it rejected every such webhook
    /// with a parse error before the idempotency record — silently dropping all
    /// cancellations and downgrades. This drives the real `construct_event`
    /// path, so it fails on 0.41 and passes on 1.0.
    #[test]
    fn subscription_deleted_webhook_verifies_and_maps_with_item_period() {
        let payload =
            subscription_event_payload("customer.subscription.deleted", "canceled", false, None);

        let secret = "whsec_test_secret";
        // Sign with the current time so the 5-minute tolerance check in
        // `verify_webhook_signature` passes; the event's own `created` field is
        // independent of the signature timestamp.
        let signature = Webhook::generate_test_header(&payload, secret, None);

        let client = StripeApiClient::new(&api_key(), webhook_secret(secret));
        let mapped = client
            .verify_webhook_signature(&payload, &signature)
            .expect("subscription.deleted should verify and parse on async-stripe 1.0");

        assert_eq!(mapped.type_, "customer.subscription.deleted");
        match mapped.kind {
            StripeEventKind::SubscriptionDeleted(sub) => {
                assert_eq!(sub.subscription_id, "sub_test123");
                assert_eq!(sub.customer_id, "cus_test123");
                assert_eq!(sub.status, DomainStatus::Canceled);
                assert_eq!(sub.price_id.as_deref(), Some("price_test123"));
                // The period must come from the item (the 2025-03-31+ location).
                assert_eq!(
                    sub.current_period_start,
                    Utc.timestamp_opt(1_700_000_000, 0).single()
                );
                assert_eq!(
                    sub.current_period_end,
                    Utc.timestamp_opt(1_701_000_000, 0).single()
                );
            }
            other => panic!("expected SubscriptionDeleted, got {other:?}"),
        }
    }

    /// A portal-initiated "cancel at period end" now arrives as
    /// `customer.subscription.updated` carrying `cancel_at` (a specific time)
    /// with `cancel_at_period_end == false`. The subscription is still `active`
    /// (entitled until the cancel date), but the pending cancellation must be
    /// surfaced so the UI shows "subscription ends on …" instead of
    /// "next renewal on …".
    #[test]
    fn subscription_updated_with_cancel_at_marks_pending_cancellation() {
        let payload = subscription_event_payload(
            "customer.subscription.updated",
            "active",
            false,
            Some(1_701_000_000),
        );
        let secret = "whsec_test_secret";
        let signature = Webhook::generate_test_header(&payload, secret, None);

        let client = StripeApiClient::new(&api_key(), webhook_secret(secret));
        let mapped = client
            .verify_webhook_signature(&payload, &signature)
            .expect("subscription.updated should verify and parse");

        match mapped.kind {
            StripeEventKind::SubscriptionUpserted(sub) => {
                assert_eq!(sub.status, DomainStatus::Active);
                assert!(
                    sub.cancel_at_period_end,
                    "a `cancel_at`-scheduled cancellation must surface as a pending cancellation"
                );
            }
            other => panic!("expected SubscriptionUpserted, got {other:?}"),
        }
    }

    /// An invalid signature must produce `InvalidWebhookSignature`, never
    /// silently succeed.
    #[test]
    fn rejects_a_webhook_with_an_invalid_signature() {
        let payload =
            r#"{"id":"evt_bad","object":"event","type":"customer.created","data":{"object":{}}}"#;
        let bad_signature =
            "t=1533204620,v1=deadbeef00000000000000000000000000000000000000000000000000000000";

        let client = StripeApiClient::new(&api_key(), webhook_secret("webhook_secret"));
        match client.verify_webhook_signature(payload, bad_signature) {
            Err(StripeError::InvalidWebhookSignature) => {}
            other => panic!("expected InvalidWebhookSignature, got {other:?}"),
        }
    }

    /// A malformed signature header (missing `t=` / `v1=`) is also rejected
    /// as invalid signature, not silently accepted.
    #[test]
    fn rejects_a_webhook_with_a_missing_signature_header() {
        let payload =
            r#"{"id":"evt_bad","object":"event","type":"customer.created","data":{"object":{}}}"#;
        let client = StripeApiClient::new(&api_key(), webhook_secret("webhook_secret"));
        match client.verify_webhook_signature(payload, "") {
            Err(StripeError::InvalidWebhookSignature) => {}
            other => panic!("expected InvalidWebhookSignature for empty header, got {other:?}"),
        }
    }

    #[test]
    fn status_mapping_covers_every_stripe_variant() {
        for (stripe, domain) in [
            (SubscriptionStatus::Active, DomainStatus::Active),
            (SubscriptionStatus::PastDue, DomainStatus::PastDue),
            (SubscriptionStatus::Canceled, DomainStatus::Canceled),
            (SubscriptionStatus::Unpaid, DomainStatus::Unpaid),
            (SubscriptionStatus::Incomplete, DomainStatus::Incomplete),
            (
                SubscriptionStatus::IncompleteExpired,
                DomainStatus::IncompleteExpired,
            ),
            (SubscriptionStatus::Trialing, DomainStatus::Trialing),
            (SubscriptionStatus::Paused, DomainStatus::Paused),
        ] {
            assert_eq!(map_status(&stripe), domain, "mapping for {stripe:?}");
        }
    }

    /// Regression guard for the "Upgrade to Paid" failure. The checkout session
    /// is created against a pre-existing `customer` with `automatic_tax`
    /// enabled, so the live Stripe API rejects it unless the request also sets
    /// `customer_update[address] = auto` ("Automatic tax calculation in
    /// Checkout requires a valid address on the Customer"). This drives the
    /// *real* request-construction path against a mock and asserts both params
    /// are present on the wire — a fake `StripeClient` can't catch this because
    /// it never builds the Stripe request.
    #[tokio::test]
    async fn checkout_session_request_saves_address_for_automatic_tax() {
        use universal_inbox::billing::{StripeCustomerId, StripePriceId};
        use universal_inbox::user::UserId;
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/checkout/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "cs_test_123",
                "object": "checkout.session",
                "url": "https://checkout.stripe.test/c/pay/cs_test_123",
            })))
            .mount(&server)
            .await;

        let client = StripeApiClient::new_with_base_url(
            &api_key(),
            webhook_secret("whsec_test"),
            &server.uri(),
        );

        // We assert on the OUTGOING request, not the (mocked) response — Stripe
        // records the request the moment it arrives, regardless of whether
        // async-stripe can fully deserialize our deliberately-minimal response.
        // So the result is intentionally ignored here.
        let _ = client
            .create_checkout_session(CheckoutSessionParams {
                customer_id: StripeCustomerId("cus_test".to_string()),
                price_id: StripePriceId("price_test".to_string()),
                success_url: "https://app.test/billing?checkout=success".parse().unwrap(),
                cancel_url: "https://app.test/billing?checkout=cancel".parse().unwrap(),
                user_id: UserId(uuid::Uuid::nil()),
            })
            .await;

        let requests = server
            .received_requests()
            .await
            .expect("mock recorded the request");
        assert_eq!(requests.len(), 1, "exactly one checkout-session request");
        // Form-decode the body so nested params (`a[b]`) and percent-encoding
        // are normalised before asserting.
        let pairs: Vec<(String, String)> = serde_urlencoded::from_bytes(&requests[0].body)
            .expect("checkout-session body is form-urlencoded");
        let has = |k: &str, v: &str| pairs.iter().any(|(pk, pv)| pk == k && pv == v);
        assert!(
            has("automatic_tax[enabled]", "true"),
            "automatic_tax must be enabled, body was: {pairs:?}"
        );
        assert!(
            has("customer_update[address]", "auto"),
            "must save the billing address onto the customer so automatic_tax can \
             compute (else Stripe 400s the upgrade), body was: {pairs:?}"
        );
    }
}
