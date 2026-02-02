use std::sync::Arc;

use actix_jwt_authc::Authenticated;
use actix_web::{HttpResponse, Scope, web};
use anyhow::Context;
use serde::{Deserialize, Serialize};

use universal_inbox::{subscription::BillingInterval, user::UserId};

use crate::{
    subscription::service::SubscriptionService, universal_inbox::UniversalInboxError,
    utils::jwt::Claims,
};

pub fn scope() -> Scope {
    web::scope("/subscriptions")
        .service(web::resource("/me").route(web::get().to(get_subscription_status)))
        .service(web::resource("/checkout").route(web::post().to(create_checkout_session)))
        .service(web::resource("/portal").route(web::post().to(create_portal_session)))
}

#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(user.id),
    err
)]
pub async fn get_subscription_status(
    subscription_service: web::Data<Arc<SubscriptionService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    tracing::Span::current().record("user.id", user_id.to_string());

    let service = subscription_service.into_inner();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while fetching subscription status")?;

    let subscription_info = service
        .get_subscription_status(&mut transaction, user_id)
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&subscription_info)
            .context("Cannot serialize subscription status")?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct CreateCheckoutSessionRequest {
    pub billing_interval: BillingIntervalRequest,
    pub success_url: String,
    pub cancel_url: String,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum BillingIntervalRequest {
    Monthly,
    Annual,
}

#[derive(Debug, Serialize)]
pub struct CheckoutSessionResponse {
    pub checkout_url: String,
}

#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(user.id, billing_interval = ?request.billing_interval),
    err
)]
pub async fn create_checkout_session(
    subscription_service: web::Data<Arc<SubscriptionService>>,
    authenticated: Authenticated<Claims>,
    request: web::Json<CreateCheckoutSessionRequest>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    tracing::Span::current().record("user.id", user_id.to_string());

    let billing_interval = match request.billing_interval {
        BillingIntervalRequest::Monthly => BillingInterval::Month,
        BillingIntervalRequest::Annual => BillingInterval::Year,
    };

    let service = subscription_service.into_inner();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating checkout session")?;

    let checkout_url = service
        .create_checkout_session(
            &mut transaction,
            user_id,
            billing_interval,
            &request.success_url,
            &request.cancel_url,
        )
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&CheckoutSessionResponse {
            checkout_url: checkout_url.to_string(),
        })
        .context("Cannot serialize checkout session response")?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct CreatePortalSessionRequest {
    pub return_url: String,
}

#[derive(Debug, Serialize)]
pub struct PortalSessionResponse {
    pub portal_url: String,
}

#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(user.id),
    err
)]
pub async fn create_portal_session(
    subscription_service: web::Data<Arc<SubscriptionService>>,
    authenticated: Authenticated<Claims>,
    request: web::Json<CreatePortalSessionRequest>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    tracing::Span::current().record("user.id", user_id.to_string());

    let service = subscription_service.into_inner();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating portal session")?;

    let portal_url = service
        .create_portal_session(&mut transaction, user_id, &request.return_url)
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&PortalSessionResponse {
            portal_url: portal_url.to_string(),
        })
        .context("Cannot serialize portal session response")?,
    ))
}
