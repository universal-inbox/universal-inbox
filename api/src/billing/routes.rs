//! HTTP routes for the billing subsystem. Mounted under `/api/billing` only
//! when `[billing]` is configured — `routes::scope` panics if called without
//! a wired-up `BillingService`, so the lib.rs hook guards via Option<>.

use std::sync::Arc;

use crate::middlewares::jwt_auth::Authenticated;
use crate::observability::attr;
use actix_web::{HttpRequest, HttpResponse, Scope, web};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::warn;
use universal_inbox::billing::{BillingStateResponse, IntegrationUsage, Plan, SyncIntervals};
use url::Url;

use crate::{
    billing::service::{BillingService, SyncKind, effective_sync_interval},
    configuration::Settings,
    universal_inbox::UniversalInboxError,
    utils::jwt::Claims,
};

/// Header carrying the Stripe webhook signature (`t=…,v1=…`). Same name and
/// format as documented at https://stripe.com/docs/webhooks/signatures.
const STRIPE_SIGNATURE_HEADER: &str = "Stripe-Signature";

pub fn scope() -> Scope {
    web::scope("/billing")
        .service(web::resource("/me").route(web::get().to(get_billing_state)))
        .service(web::resource("/checkout-session").route(web::post().to(create_checkout_session)))
        .service(web::resource("/portal-session").route(web::post().to(create_portal_session)))
        .service(web::resource("/stripe/webhook").route(web::post().to(stripe_webhook)))
}

#[derive(Debug, Deserialize)]
pub struct CheckoutSessionRequest {
    pub success_url: Url,
    pub cancel_url: Url,
}

#[derive(Debug, Deserialize)]
pub struct PortalSessionRequest {
    pub return_url: Url,
}

#[derive(Debug, Serialize)]
pub struct SessionUrlResponse {
    pub url: Url,
}

/// Refuse a Stripe redirect target that does not point back at this instance's
/// own frontend.
///
/// These URLs arrive in the request body typed only as a `Url`, so any
/// absolute URL parses. Unchecked, they make the billing routes an
/// open-redirect generator: a genuine `checkout.stripe.com` link whose
/// cancel/return control lands the visitor on a site of the caller's choosing.
/// Scheme, host and effective port must match `front_base_url`.
fn ensure_returns_to_frontend(
    url: &Url,
    front_base_url: &Url,
    field: &str,
) -> Result<(), UniversalInboxError> {
    let same_origin = url.scheme() == front_base_url.scheme()
        && url.host_str() == front_base_url.host_str()
        && url.port_or_known_default() == front_base_url.port_or_known_default();

    if same_origin {
        Ok(())
    } else {
        Err(UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("`{field}` must point to {front_base_url}"),
        })
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_billing_state(
    billing_service: web::Data<Arc<BillingService>>,
    settings: web::Data<Settings>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    use crate::billing::repository::BillingRepository;
    let user_id = authenticated.user_id()?;

    let mut transaction = billing_service
        .begin()
        .await
        .context("Failed to create transaction while loading billing state")?;

    let limits = billing_service.limits().clone();

    // Lazily arm the over-limit grace deadline on read. Idempotent: a no-op for
    // Paid users, users under the cap, or an already-armed deadline. This is
    // where the otherwise-dead grace path gets wired — the web banner and
    // billing-page warning render directly off `over_limit_grace_deadline`, so
    // arming here lights them up exactly when first shown.
    billing_service
        .mark_over_limit_grace_deadline_if_needed(&mut transaction, user_id)
        .await?;

    // Single subscription fetch (post-arming, so a freshly-armed deadline is
    // reflected). The plan is derived from this row instead of a second
    // `get_user_plan` round-trip on the same table.
    let sub = billing_service
        .repository
        .get_user_subscription(&mut transaction, user_id)
        .await?;
    let plan = sub
        .as_ref()
        .map(|s| s.effective_plan())
        .unwrap_or(Plan::Free);

    let used = billing_service
        .integration_counter
        .count_validated_for_user(&mut transaction, user_id)
        .await?;
    let paused_by_plan = billing_service
        .integration_counter
        .count_plan_paused_for_user(&mut transaction, user_id)
        .await?;

    let cancel_at_period_end = sub
        .as_ref()
        .map(|s| s.cancel_at_period_end)
        .unwrap_or(false);
    let current_period_end = sub.as_ref().and_then(|s| s.current_period_end);
    let over_limit_grace_deadline = sub.as_ref().and_then(|s| s.over_limit_grace_deadline);

    let app = &settings.application;
    let response = BillingStateResponse {
        plan,
        status: sub.as_ref().map(|s| s.status),
        current_period_end,
        cancel_at_period_end,
        integration_usage: IntegrationUsage {
            used,
            limit: if plan.is_paid() {
                None
            } else {
                Some(limits.max_integration_connections)
            },
            paused_by_plan,
        },
        // Report the interval actually enforced — `max(global, plan)` for Free,
        // the global floor for Paid — not the raw plan floor or a misleading 0.
        sync_interval: SyncIntervals {
            notification_minutes: effective_sync_interval(
                &limits,
                plan,
                app.min_sync_notifications_interval_in_minutes,
                SyncKind::Notifications,
            ),
            task_minutes: effective_sync_interval(
                &limits,
                plan,
                app.min_sync_tasks_interval_in_minutes,
                SyncKind::Tasks,
            ),
        },
        upgrade_available: !plan.is_paid(),
        manage_billing_available: sub.as_ref().is_some_and(|s| s.stripe_customer_id.is_some()),
        over_limit_grace_deadline,
    };

    transaction
        .commit()
        .await
        .context("Failed to commit billing-state transaction")?;

    Ok(HttpResponse::Ok().json(response))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn create_checkout_session(
    body: web::Json<CheckoutSessionRequest>,
    billing_service: web::Data<Arc<BillingService>>,
    settings: web::Data<Settings>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated.user_id()?;
    let front_base_url = &settings.application.front_base_url;
    ensure_returns_to_frontend(&body.success_url, front_base_url, "success_url")?;
    ensure_returns_to_frontend(&body.cancel_url, front_base_url, "cancel_url")?;

    let mut transaction = billing_service
        .begin()
        .await
        .context("Failed to create transaction while starting checkout session")?;
    let url = billing_service
        .create_checkout_session(
            &mut transaction,
            user_id,
            body.success_url.clone(),
            body.cancel_url.clone(),
        )
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit checkout-session transaction")?;

    Ok(HttpResponse::Ok().json(SessionUrlResponse { url }))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn create_portal_session(
    body: web::Json<PortalSessionRequest>,
    billing_service: web::Data<Arc<BillingService>>,
    settings: web::Data<Settings>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated.user_id()?;
    ensure_returns_to_frontend(
        &body.return_url,
        &settings.application.front_base_url,
        "return_url",
    )?;

    let mut transaction = billing_service
        .begin()
        .await
        .context("Failed to create transaction while starting billing portal session")?;
    let url = billing_service
        .create_portal_session(&mut transaction, user_id, body.return_url.clone())
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit portal-session transaction")?;

    Ok(HttpResponse::Ok().json(SessionUrlResponse { url }))
}

#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(
        { attr::STRIPE_EVENT_ID } = tracing::field::Empty,
        { attr::STRIPE_EVENT_TYPE } = tracing::field::Empty
    ),
    err
)]
pub async fn stripe_webhook(
    req: HttpRequest,
    body: web::Bytes,
    billing_service: web::Data<Arc<BillingService>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let signature = match req
        .headers()
        .get(STRIPE_SIGNATURE_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        Some(sig) => sig,
        None => {
            warn!("Rejected Stripe webhook with missing signature header");
            return Ok(HttpResponse::BadRequest().json(json!({
                "message": "Missing Stripe-Signature header"
            })));
        }
    };

    let payload = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(err) => {
            warn!("Rejected Stripe webhook with non-utf8 payload: {err}");
            return Ok(HttpResponse::BadRequest().json(json!({
                "message": "Invalid webhook payload"
            })));
        }
    };

    let event = match billing_service
        .stripe
        .verify_webhook_signature(payload, signature)
    {
        Ok(e) => e,
        Err(err) => {
            warn!("Rejected invalid Stripe webhook: {err}");
            return Ok(HttpResponse::BadRequest().json(json!({
                "message": "Invalid Stripe webhook signature"
            })));
        }
    };
    let current_span = tracing::Span::current();
    current_span.record(attr::STRIPE_EVENT_ID, event.id.as_str());
    current_span.record(attr::STRIPE_EVENT_TYPE, event.type_.as_str());

    use crate::billing::repository::BillingRepository;
    use crate::billing::stripe::StripeEventKind;

    // Fetch any subscription state this event needs from Stripe *before*
    // opening the transaction. Holding an open Postgres transaction across a
    // synchronous Stripe call lets a Stripe latency spike pin a DB connection
    // per in-flight delivery during a webhook burst, exhausting the pool and
    // stalling unrelated traffic. Doing the (read-only) fetch up front keeps
    // the transaction free of network I/O while preserving the
    // rollback-on-failure → Stripe-retry property (the idempotency row is
    // written in the same transaction as the apply).
    let subscription_to_fetch = match &event.kind {
        StripeEventKind::CheckoutSessionCompleted {
            subscription_id: Some(id),
            ..
        }
        | StripeEventKind::InvoicePaid {
            subscription_id: Some(id),
            ..
        }
        | StripeEventKind::InvoicePaymentFailed {
            subscription_id: Some(id),
            ..
        } => Some(id.clone()),
        _ => None,
    };
    let prefetched = match subscription_to_fetch {
        Some(sub_id) => Some(billing_service.stripe.fetch_subscription(&sub_id).await?),
        None => None,
    };

    let mut transaction = billing_service
        .begin()
        .await
        .context("Failed to create transaction while processing Stripe webhook")?;

    let newly_recorded = billing_service
        .repository
        .record_stripe_event(&mut transaction, &event.id, &event.type_)
        .await?;

    if !newly_recorded {
        // Idempotent re-delivery — already processed.
        transaction
            .commit()
            .await
            .context("Failed to commit no-op webhook transaction")?;
        return Ok(HttpResponse::Ok().finish());
    }

    billing_service
        .apply_subscription_event(&mut transaction, &event, prefetched)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit Stripe webhook transaction")?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    fn front() -> Url {
        Url::parse("https://app.universal-inbox.com/").unwrap()
    }

    #[rstest]
    #[case("https://app.universal-inbox.com/billing?checkout=success")]
    #[case("https://app.universal-inbox.com/")]
    #[case("https://app.universal-inbox.com:443/billing")]
    fn accepts_a_url_on_the_frontend_origin(#[case] url: &str) {
        let url = Url::parse(url).unwrap();

        assert!(ensure_returns_to_frontend(&url, &front(), "success_url").is_ok());
    }

    #[rstest]
    // A look-alike host is the phishing case the check exists for.
    #[case("https://app.universal-inbox.com.evil.test/billing")]
    #[case("https://evil.test/login")]
    // Downgrading the scheme or moving the port is still another origin.
    #[case("http://app.universal-inbox.com/billing")]
    #[case("https://app.universal-inbox.com:8443/billing")]
    // A scheme Stripe would hand to the browser as-is.
    #[case("javascript:alert(1)")]
    fn refuses_a_url_off_the_frontend_origin(#[case] url: &str) {
        let url = Url::parse(url).unwrap();

        let error = ensure_returns_to_frontend(&url, &front(), "success_url")
            .expect_err("a foreign origin must be refused");

        assert!(matches!(
            error,
            UniversalInboxError::InvalidInputData { .. }
        ));
    }
}
