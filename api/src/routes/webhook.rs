use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, Scope, web};
use anyhow::Context;
use apalis::prelude::Storage;
use apalis_redis::RedisStorage;
use ring::hmac;
use secrecy::{ExposeSecret, SecretBox};
use serde_json::json;
use slack_morphism::prelude::*;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use tokio_retry::{
    Retry,
    strategy::{ExponentialBackoff, jitter},
};
use tracing::{debug, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackReactionConfig},
        provider::IntegrationProviderKind,
    },
    third_party::item::ThirdPartyItemKind,
};

use crate::{
    configuration::WebhookSigningSecret,
    integrations::slack::has_slack_references_in_message,
    jobs::{UniversalInboxJob, slack::SlackPushEventCallbackJob},
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        third_party::service::ThirdPartyItemService,
    },
};

pub type SlackSigningSecret = SecretBox<WebhookSigningSecret>;

const SLACK_SIGNATURE_HEADER: &str = "X-Slack-Signature";
const SLACK_TIMESTAMP_HEADER: &str = "X-Slack-Request-Timestamp";
const SLACK_SIGNATURE_TOLERANCE_SECONDS: i64 = 300;

pub fn scope() -> Scope {
    web::scope("/hooks")
        .service(web::resource("/slack/events").route(web::post().to(push_slack_event)))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn push_slack_event(
    req: HttpRequest,
    body: web::Bytes,
    signing_secret: web::Data<Option<SlackSigningSecret>>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    third_party_item_service: web::Data<Arc<RwLock<ThirdPartyItemService>>>,
    storage: web::Data<RedisStorage<UniversalInboxJob>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let current_span = tracing::Span::current();

    let Some(signing_secret) = signing_secret.as_ref() else {
        // Boot guard should make this unreachable; defensive 401 just in case.
        warn!("Rejected Slack webhook: no signing secret configured at runtime");
        return Ok(HttpResponse::Unauthorized().finish());
    };
    if let Err(reason) = verify_slack_signature(&req, &body, &signing_secret.expose_secret().0) {
        warn!(reason, "Rejected unsigned/invalid Slack webhook");
        return Ok(HttpResponse::Unauthorized().finish());
    }

    let slack_push_event: SlackPushEvent = serde_json::from_slice(&body)
        .context("Failed to deserialize Slack push event after signature verification")?;

    match slack_push_event {
        SlackPushEvent::UrlVerification(SlackUrlVerificationEvent { challenge }) => {
            current_span.set_attribute("slack.event_type", "url_verification");
            current_span.set_attribute("slack.event.outcome", "url_verification");
            return Ok(HttpResponse::Ok()
                .content_type("application/json")
                .body(json!({ "challenge": challenge }).to_string()));
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
            let event_type = match &event.event {
                SlackEventCallbackBody::ReactionAdded(_) => "reaction_added",
                SlackEventCallbackBody::ReactionRemoved(_) => "reaction_removed",
                _ => unreachable!(),
            };
            current_span.set_attribute("slack.event_type", event_type);
            current_span.set_attribute("slack.team_id", event.team_id.to_string());
            current_span.set_attribute("slack.event_id", event.event_id.to_string());
            current_span.set_attribute("slack.user_id", user.to_string());
            current_span.set_attribute("slack.reaction", reaction.to_string());

            let service = integration_connection_service.read().await;
            let mut transaction = service
                .begin()
                .await
                .context("Failed to create new transaction while checking Slack user ID")?;

            let config = service
                .get_integration_connection_config_for_provider_user_id(
                    &mut transaction,
                    IntegrationProviderKind::Slack,
                    user.to_string(),
                )
                .await?;

            match config {
                Some(IntegrationConnectionConfig::Slack(SlackConfig {
                    reaction_config:
                        SlackReactionConfig {
                            sync_enabled: true,
                            reaction_name,
                            ..
                        },
                    ..
                })) if reaction_name == *reaction => {
                    current_span.set_attribute("slack.event.outcome", "queued");
                    send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
                }
                Some(IntegrationConnectionConfig::Slack(SlackConfig {
                    reaction_config:
                        SlackReactionConfig {
                            sync_enabled: false,
                            ..
                        },
                    ..
                })) => {
                    current_span.set_attribute("slack.event.outcome", "discarded");
                    current_span
                        .set_attribute("slack.event.discard_reason", "reaction_sync_disabled");
                }
                Some(IntegrationConnectionConfig::Slack(_)) => {
                    current_span.set_attribute("slack.event.outcome", "discarded");
                    current_span
                        .set_attribute("slack.event.discard_reason", "reaction_name_mismatch");
                }
                _ => {
                    current_span.set_attribute("slack.event.outcome", "discarded");
                    current_span.set_attribute("slack.event.discard_reason", "no_slack_config");
                }
            }
        }
        SlackPushEvent::EventCallback(
            ref event @ SlackPushEventCallback {
                event:
                    SlackEventCallbackBody::Message(SlackMessageEvent {
                        origin:
                            SlackMessageOrigin {
                                ref thread_ts,
                                ref ts,
                                ref channel,
                                ..
                            },
                        content: Some(ref content),
                        sender: SlackMessageSender { ref user, .. },
                        ..
                    }),
                ..
            },
        ) => {
            current_span.set_attribute("slack.event_type", "message");
            current_span.set_attribute("slack.team_id", event.team_id.to_string());
            current_span.set_attribute("slack.event_id", event.event_id.to_string());
            current_span.set_attribute("slack.ts", ts.to_string());
            if let Some(thread_ts) = thread_ts {
                current_span.set_attribute("slack.thread_ts", thread_ts.to_string());
            }
            if let Some(channel) = channel {
                current_span.set_attribute("slack.channel_id", channel.to_string());
            }
            if let Some(user) = user {
                current_span.set_attribute("slack.user_id", user.to_string());
            }

            let service = third_party_item_service.read().await;
            let mut transaction = service.begin().await.context(
                "Failed to create new transaction while checking for known Slack threads",
            )?;

            if has_slack_references_in_message(content) {
                current_span.set_attribute("slack.event.outcome", "queued");
                current_span.set_attribute("slack.event.queue_reason", "has_references");
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
                current_span.set_attribute("slack.event.outcome", "queued");
                current_span.set_attribute("slack.event.queue_reason", "known_thread");
                send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
                return Ok(HttpResponse::Ok().finish());
            }

            current_span.set_attribute("slack.event.outcome", "discarded");
            current_span.set_attribute(
                "slack.event.discard_reason",
                "no_references_no_known_thread",
            );
        }
        SlackPushEvent::AppRateLimited(SlackAppRateLimitedEvent {
            team_id,
            minute_rate_limited,
            api_app_id,
        }) => {
            current_span.set_attribute("slack.event_type", "app_rate_limited");
            current_span.set_attribute("slack.team_id", team_id.to_string());
            current_span.set_attribute("slack.event.outcome", "rate_limited");
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
            current_span.set_attribute("slack.event_type", "unknown");
            current_span.set_attribute("slack.team_id", team_id.to_string());
            current_span.set_attribute("slack.event_id", event_id.to_string());
            current_span.set_attribute("slack.event.outcome", "discarded");
            current_span.set_attribute("slack.event.discard_reason", "unknown_event_type");
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

/// Verify a Slack webhook signature per
/// <https://api.slack.com/authentication/verifying-requests-from-slack>.
///
/// Reads `X-Slack-Request-Timestamp` and `X-Slack-Signature` from `req.headers()`,
/// then delegates to the pure [`verify_slack_signature_parts`] helper. Splitting
/// header extraction from verification keeps the cryptographic logic unit-testable
/// without spinning up an Actix request.
fn verify_slack_signature(
    req: &HttpRequest,
    body: &[u8],
    signing_secret: &str,
) -> Result<(), &'static str> {
    let headers = req.headers();
    let timestamp_header = headers
        .get(SLACK_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok());
    let signature_header = headers
        .get(SLACK_SIGNATURE_HEADER)
        .and_then(|v| v.to_str().ok());
    verify_slack_signature_parts(
        timestamp_header,
        signature_header,
        body,
        signing_secret,
        chrono::Utc::now().timestamp(),
    )
}

/// Pure signature-verification logic — no Actix, no clock.
///
/// Rejects timestamps outside a ±5min window of `now_unix` (replay protection),
/// recomputes `HMAC-SHA256(signing_secret, "v0:<ts>:<body>")`, and constant-time
/// compares against the `v0=`-prefixed hex signature.
fn verify_slack_signature_parts(
    timestamp_header: Option<&str>,
    signature_header: Option<&str>,
    body: &[u8],
    signing_secret: &str,
    now_unix: i64,
) -> Result<(), &'static str> {
    let timestamp = timestamp_header.ok_or("missing timestamp header")?;
    let signature = signature_header.ok_or("missing signature header")?;

    let ts: i64 = timestamp.parse().map_err(|_| "non-numeric timestamp")?;
    if (now_unix - ts).abs() > SLACK_SIGNATURE_TOLERANCE_SECONDS {
        return Err("stale timestamp");
    }

    let Some(provided_hex) = signature.strip_prefix("v0=") else {
        return Err("missing v0 prefix");
    };
    let provided_bytes = hex::decode(provided_hex).map_err(|_| "non-hex signature")?;

    let mut basestring = Vec::with_capacity(3 + timestamp.len() + 1 + body.len());
    basestring.extend_from_slice(b"v0:");
    basestring.extend_from_slice(timestamp.as_bytes());
    basestring.push(b':');
    basestring.extend_from_slice(body);

    let key = hmac::Key::new(hmac::HMAC_SHA256, signing_secret.as_bytes());
    let expected = hmac::sign(&key, &basestring);

    if expected.as_ref().ct_eq(&provided_bytes).into() {
        Ok(())
    } else {
        Err("signature mismatch")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-signing-secret-do-not-use-in-prod";
    const NOW: i64 = 1_700_000_000;

    /// Build a valid `v0=<hex>` signature for the given (ts, body) using `SECRET`.
    fn sign(timestamp: &str, body: &[u8]) -> String {
        let mut basestring = Vec::with_capacity(3 + timestamp.len() + 1 + body.len());
        basestring.extend_from_slice(b"v0:");
        basestring.extend_from_slice(timestamp.as_bytes());
        basestring.push(b':');
        basestring.extend_from_slice(body);
        let key = hmac::Key::new(hmac::HMAC_SHA256, SECRET.as_bytes());
        format!("v0={}", hex::encode(hmac::sign(&key, &basestring).as_ref()))
    }

    #[test]
    fn accepts_valid_signature() {
        let body = b"{\"type\":\"url_verification\"}";
        let ts = NOW.to_string();
        let sig = sign(&ts, body);
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some(&sig), body, SECRET, NOW),
            Ok(())
        );
    }

    #[test]
    fn accepts_signature_at_tolerance_edge() {
        let body = b"{}";
        let ts = (NOW - SLACK_SIGNATURE_TOLERANCE_SECONDS).to_string();
        let sig = sign(&ts, body);
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some(&sig), body, SECRET, NOW),
            Ok(())
        );
    }

    #[test]
    fn rejects_missing_timestamp() {
        let body = b"{}";
        let sig = sign(&NOW.to_string(), body);
        assert_eq!(
            verify_slack_signature_parts(None, Some(&sig), body, SECRET, NOW),
            Err("missing timestamp header")
        );
    }

    #[test]
    fn rejects_missing_signature() {
        let body = b"{}";
        assert_eq!(
            verify_slack_signature_parts(Some(&NOW.to_string()), None, body, SECRET, NOW),
            Err("missing signature header")
        );
    }

    #[test]
    fn rejects_non_numeric_timestamp() {
        let body = b"{}";
        let sig = sign("not-a-number", body);
        assert_eq!(
            verify_slack_signature_parts(Some("not-a-number"), Some(&sig), body, SECRET, NOW),
            Err("non-numeric timestamp")
        );
    }

    #[test]
    fn rejects_stale_timestamp_past() {
        let body = b"{}";
        // 1 second past the tolerance — signature still valid for that timestamp.
        let ts = (NOW - SLACK_SIGNATURE_TOLERANCE_SECONDS - 1).to_string();
        let sig = sign(&ts, body);
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some(&sig), body, SECRET, NOW),
            Err("stale timestamp")
        );
    }

    #[test]
    fn rejects_stale_timestamp_future() {
        let body = b"{}";
        // Far in the future — guards against clock-skew abuse on either side.
        let ts = (NOW + SLACK_SIGNATURE_TOLERANCE_SECONDS + 1).to_string();
        let sig = sign(&ts, body);
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some(&sig), body, SECRET, NOW),
            Err("stale timestamp")
        );
    }

    #[test]
    fn rejects_missing_v0_prefix() {
        let body = b"{}";
        let ts = NOW.to_string();
        let bare_hex = sign(&ts, body).strip_prefix("v0=").unwrap().to_string();
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some(&bare_hex), body, SECRET, NOW),
            Err("missing v0 prefix")
        );
    }

    #[test]
    fn rejects_non_hex_signature() {
        let body = b"{}";
        let ts = NOW.to_string();
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some("v0=not-hex!!"), body, SECRET, NOW),
            Err("non-hex signature")
        );
    }

    #[test]
    fn rejects_signature_for_different_body() {
        let ts = NOW.to_string();
        let sig = sign(&ts, b"original-body");
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some(&sig), b"tampered-body", SECRET, NOW),
            Err("signature mismatch")
        );
    }

    #[test]
    fn rejects_signature_for_different_secret() {
        let body = b"{}";
        let ts = NOW.to_string();
        let sig = sign(&ts, body);
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some(&sig), body, "different-secret", NOW),
            Err("signature mismatch")
        );
    }
}
