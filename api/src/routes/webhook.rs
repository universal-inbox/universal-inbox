use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, Scope, web};
use anyhow::Context;
use apalis::prelude::Storage;
use apalis_redis::RedisStorage;
use chrono::{TimeZone, Utc};
use secrecy::ExposeSecret;
use serde_json::json;
use slack_morphism::prelude::*;
use stripe::{EventObject, EventType, Webhook};
use tokio::sync::RwLock;
use tokio_retry::{
    Retry,
    strategy::{ExponentialBackoff, jitter},
};
use tracing::{debug, info, warn};
use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackReactionConfig, SlackStarConfig},
        provider::IntegrationProviderKind,
    },
    subscription::BillingInterval,
    third_party::item::ThirdPartyItemKind,
};

use crate::{
    configuration::Settings,
    integrations::slack::has_slack_references_in_message,
    jobs::{UniversalInboxJob, slack::SlackPushEventCallbackJob},
    subscription::service::SubscriptionService,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        third_party::service::ThirdPartyItemService,
    },
};

pub fn scope() -> Scope {
    web::scope("/hooks")
        .service(web::resource("/slack/events").route(web::post().to(push_slack_event)))
        .service(web::resource("/stripe").route(web::post().to(handle_stripe_webhook)))
}

#[tracing::instrument(level = "debug", skip_all, fields(event_type), err)]
pub async fn handle_stripe_webhook(
    req: HttpRequest,
    payload: web::Bytes,
    subscription_service: web::Data<Arc<RwLock<SubscriptionService>>>,
    settings: web::Data<Settings>,
) -> Result<HttpResponse, UniversalInboxError> {
    let Some(webhook_secret) = settings.stripe.webhook_secret.as_ref() else {
        warn!("Stripe webhook received but webhook_secret is not configured");
        return Ok(HttpResponse::Ok().finish());
    };

    let payload_str =
        std::str::from_utf8(&payload).map_err(|_| UniversalInboxError::InvalidInputData {
            source: None,
            user_error: "Invalid UTF-8 in request body".to_string(),
        })?;

    let stripe_signature = req
        .headers()
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let event = Webhook::construct_event(
        payload_str,
        stripe_signature,
        &webhook_secret.expose_secret().0,
    )
    .map_err(|e| {
        warn!("Failed to verify Stripe webhook signature: {e:?}");
        UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("Invalid webhook signature: {e:?}"),
        }
    })?;

    tracing::Span::current().record("event_type", event.type_.to_string());
    info!(
        event_id = %event.id,
        event_type = %event.type_,
        "Received Stripe webhook event"
    );

    let service = subscription_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction for Stripe webhook")?;

    match event.type_ {
        EventType::CheckoutSessionCompleted => {
            if let EventObject::CheckoutSession(session) = event.data.object
                && let (Some(customer_id), Some(subscription_id)) = (
                    session.customer.map(|c| c.id().to_string()),
                    session.subscription.map(|s| s.id().to_string()),
                )
            {
                let stripe_subscription = service
                    .stripe_service()
                    .ok_or_else(|| {
                        UniversalInboxError::Unexpected(anyhow::anyhow!(
                            "Stripe service not configured"
                        ))
                    })?
                    .get_subscription(&subscription_id)
                    .await?;

                service
                    .handle_checkout_session_completed(
                        &mut transaction,
                        &customer_id,
                        &subscription_id,
                        &stripe_subscription.status,
                        stripe_subscription.current_period_end,
                        stripe_subscription.billing_interval,
                    )
                    .await?;

                info!(
                    customer_id = %customer_id,
                    subscription_id = %subscription_id,
                    "Processed checkout.session.completed"
                );
            }
        }
        EventType::CustomerSubscriptionUpdated => {
            if let EventObject::Subscription(subscription) = event.data.object {
                let subscription_id = subscription.id.to_string();
                let current_period_end = Utc
                    .timestamp_opt(subscription.current_period_end, 0)
                    .single();
                let billing_interval = subscription
                    .items
                    .data
                    .first()
                    .and_then(|item| item.price.as_ref())
                    .and_then(|price| price.recurring.as_ref())
                    .map(|recurring| match recurring.interval {
                        stripe::RecurringInterval::Month => BillingInterval::Month,
                        stripe::RecurringInterval::Year => BillingInterval::Year,
                        _ => BillingInterval::Month,
                    });

                service
                    .handle_subscription_updated(
                        &mut transaction,
                        &subscription_id,
                        &subscription.status,
                        current_period_end,
                        billing_interval,
                    )
                    .await?;

                info!(
                    subscription_id = %subscription_id,
                    status = ?subscription.status,
                    "Processed customer.subscription.updated"
                );
            }
        }
        EventType::CustomerSubscriptionDeleted => {
            if let EventObject::Subscription(subscription) = event.data.object {
                let subscription_id = subscription.id.to_string();

                service
                    .handle_subscription_deleted(&mut transaction, &subscription_id)
                    .await?;

                info!(
                    subscription_id = %subscription_id,
                    "Processed customer.subscription.deleted"
                );
            }
        }
        EventType::InvoicePaymentFailed => {
            if let EventObject::Invoice(invoice) = event.data.object
                && let Some(subscription_id) = invoice.subscription.map(|s| s.id().to_string())
            {
                service
                    .handle_invoice_payment_failed(&mut transaction, &subscription_id)
                    .await?;

                info!(
                    subscription_id = %subscription_id,
                    "Processed invoice.payment_failed"
                );
            }
        }
        EventType::InvoicePaid => {
            if let EventObject::Invoice(invoice) = event.data.object
                && let Some(subscription_id) = invoice.subscription.map(|s| s.id().to_string())
            {
                service
                    .handle_invoice_paid(&mut transaction, &subscription_id)
                    .await?;

                info!(
                    subscription_id = %subscription_id,
                    "Processed invoice.paid"
                );
            }
        }
        _ => {
            debug!(
                event_type = %event.type_,
                "Ignoring unhandled Stripe webhook event type"
            );
        }
    }

    transaction
        .commit()
        .await
        .context("Failed to commit transaction for Stripe webhook")?;

    Ok(HttpResponse::Ok().finish())
}

pub async fn push_slack_event(
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    third_party_item_service: web::Data<Arc<RwLock<ThirdPartyItemService>>>,
    slack_push_event: web::Json<SlackPushEvent>,
    storage: web::Data<RedisStorage<UniversalInboxJob>>,
) -> Result<HttpResponse, UniversalInboxError> {
    match slack_push_event.into_inner() {
        SlackPushEvent::UrlVerification(SlackUrlVerificationEvent { challenge }) => {
            return Ok(HttpResponse::Ok()
                .content_type("application/json")
                .body(json!({ "challenge": challenge }).to_string()));
        }
        SlackPushEvent::EventCallback(
            ref event @ SlackPushEventCallback {
                event: SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { ref user, .. }),
                ..
            },
        )
        | SlackPushEvent::EventCallback(
            ref event @ SlackPushEventCallback {
                event: SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { ref user, .. }),
                ..
            },
        ) => {
            let service = integration_connection_service.read().await;
            let mut transaction = service
                .begin()
                .await
                .context("Failed to create new transaction while checking Slack user ID")?;

            if let Some(IntegrationConnectionConfig::Slack(SlackConfig {
                star_config:
                    SlackStarConfig {
                        sync_enabled: true, ..
                    },
                ..
            })) = service
                .get_integration_connection_config_for_provider_user_id(
                    &mut transaction,
                    IntegrationProviderKind::Slack,
                    user.to_string(),
                )
                .await?
            {
                send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
            }
        }
        SlackPushEvent::EventCallback(
            ref event @ SlackPushEventCallback {
                event:
                    SlackEventCallbackBody::ReactionAdded(SlackReactionAddedEvent {
                        ref user,
                        ref reaction,
                        ..
                    }),
                ..
            },
        )
        | SlackPushEvent::EventCallback(
            ref event @ SlackPushEventCallback {
                event:
                    SlackEventCallbackBody::ReactionRemoved(SlackReactionRemovedEvent {
                        ref user,
                        ref reaction,
                        ..
                    }),
                ..
            },
        ) => {
            let service = integration_connection_service.read().await;
            let mut transaction = service
                .begin()
                .await
                .context("Failed to create new transaction while checking Slack user ID")?;

            if let Some(IntegrationConnectionConfig::Slack(SlackConfig {
                reaction_config:
                    SlackReactionConfig {
                        sync_enabled: true,
                        reaction_name,
                        ..
                    },
                ..
            })) = service
                .get_integration_connection_config_for_provider_user_id(
                    &mut transaction,
                    IntegrationProviderKind::Slack,
                    user.to_string(),
                )
                .await?
                && reaction_name == *reaction
            {
                send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
            }
        }
        SlackPushEvent::EventCallback(
            ref event @ SlackPushEventCallback {
                event:
                    SlackEventCallbackBody::Message(SlackMessageEvent {
                        origin: SlackMessageOrigin { ref thread_ts, .. },
                        content: Some(ref content),
                        ..
                    }),
                ..
            },
        ) => {
            //
            let service = third_party_item_service.read().await;
            let mut transaction = service.begin().await.context(
                "Failed to create new transaction while checking for known Slack threads",
            )?;

            if has_slack_references_in_message(content) {
                send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
                return Ok(HttpResponse::Ok().finish());
            }

            // Check if the message is a reply to a known thread
            if let Some(thread_ts) = &thread_ts
                && service
                    .has_third_party_item_for_source_id(
                        &mut transaction,
                        ThirdPartyItemKind::SlackThread,
                        &thread_ts.0,
                    )
                    .await?
            {
                send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
                return Ok(HttpResponse::Ok().finish());
            }
        }
        SlackPushEvent::AppRateLimited(SlackAppRateLimitedEvent {
            team_id,
            minute_rate_limited,
            api_app_id,
        }) => {
            warn!(
                ?team_id,
                ?api_app_id,
                ?minute_rate_limited,
                "Slack pushed events are rate limited"
            );
        }
        SlackPushEvent::EventCallback(SlackPushEventCallback {
            team_id,
            api_app_id,
            event_id,
            ..
        }) => {
            warn!(
                ?team_id,
                ?api_app_id,
                ?event_id,
                "Received an unknown push event from Slack"
            );
        }
    }

    Ok(HttpResponse::Ok().finish())
}

async fn send_slack_push_event_callback_job(
    storage: &RedisStorage<UniversalInboxJob>,
    event: SlackPushEventCallback,
) -> Result<(), UniversalInboxError> {
    let job = Retry::spawn(
        ExponentialBackoff::from_millis(10).map(jitter).take(10),
        || async {
            storage
                .clone()
                .push(UniversalInboxJob::SlackPushEventCallback(
                    SlackPushEventCallbackJob(event.clone()),
                ))
                .await
        },
    )
    .await
    .context("Failed to push Slack event to queue")?;
    debug!(
        "Pushed a Slack event {} to the queue with job ID {}",
        event.event_id, job.task_id
    );
    Ok(())
}
