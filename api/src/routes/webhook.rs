use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, Scope, web};
use anyhow::Context;

use ring::hmac;
use secrecy::{ExposeSecret, SecretBox};
use serde::Deserialize;
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
    audit::AuditActor,
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackReactionConfig},
        provider::IntegrationProviderKind,
    },
    third_party::item::ThirdPartyItemKind,
};

use crate::observability::attr;
use crate::{
    configuration::WebhookSigningSecret,
    integrations::{notion::webhook::NotionWebhookEvent, slack::has_slack_references_in_message},
    jobs::{
        JobStorage, UniversalInboxJob, notion::NotionWebhookEventJob, push_job,
        slack::SlackPushEventCallbackJob,
    },
    universal_inbox::{
        UniversalInboxError, audit::actor_context::with_actor,
        integration_connection::service::IntegrationConnectionService,
        third_party::service::ThirdPartyItemService,
    },
};

pub type SlackSigningSecret = SecretBox<WebhookSigningSecret>;

/// The Notion webhook verification token. A distinct type so it does not
/// collide with Slack's secret in the Actix app data. `None` until the
/// subscription handshake is done and the token is configured.
pub struct NotionSigningSecret(pub Option<SecretBox<WebhookSigningSecret>>);

const NOTION_SIGNATURE_HEADER: &str = "X-Notion-Signature";

const SLACK_SIGNATURE_HEADER: &str = "X-Slack-Signature";
const SLACK_TIMESTAMP_HEADER: &str = "X-Slack-Request-Timestamp";
const SLACK_SIGNATURE_TOLERANCE_SECONDS: i64 = 300;

pub fn scope() -> Scope {
    web::scope("/hooks")
        .service(web::resource("/slack/events").route(web::post().to(push_slack_event)))
        .service(web::resource("/notion/events").route(web::post().to(push_notion_event)))
}

/// The one-time request Notion sends when a webhook subscription is created.
#[derive(Deserialize)]
struct NotionVerificationRequest {
    verification_token: String,
}

/// Notion webhook. Only `comment.*` events of a workspace where a user has a
/// validated Notion connection become a job: everything else is dropped here,
/// before any Notion API call.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn push_notion_event(
    req: HttpRequest,
    body: web::Bytes,
    signing_secret: web::Data<NotionSigningSecret>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    storage: web::Data<JobStorage>,
) -> Result<HttpResponse, UniversalInboxError> {
    let current_span = tracing::Span::current();

    let Some(signing_secret) = signing_secret.0.as_ref() else {
        // Until the token is configured, only the subscription handshake is
        // accepted. Its token is logged so it can be pasted in the Notion
        // dashboard and set as the signing secret.
        if let Ok(verification) = serde_json::from_slice::<NotionVerificationRequest>(&body) {
            warn!(
                verification_token = verification.verification_token,
                "Received the Notion webhook verification token: set it as \
                 UNIVERSAL_INBOX__INTEGRATIONS__NOTION__SIGNING_SECRET"
            );
            current_span.set_attribute(attr::NOTION_EVENT_OUTCOME, "verification");
            return Ok(HttpResponse::Ok().finish());
        }
        current_span.set_attribute(attr::NOTION_EVENT_OUTCOME, "rejected");
        current_span.set_attribute(attr::NOTION_EVENT_REJECT_REASON, "no_signing_secret");
        warn!("Rejected Notion webhook: no signing secret configured");
        return Ok(HttpResponse::Unauthorized().finish());
    };

    let signature_header = req
        .headers()
        .get(NOTION_SIGNATURE_HEADER)
        .and_then(|value| value.to_str().ok());
    if let Err(reason) =
        verify_notion_signature_parts(signature_header, &body, &signing_secret.expose_secret().0)
    {
        if serde_json::from_slice::<NotionVerificationRequest>(&body).is_ok() {
            // A new handshake while a token is configured: acknowledged, never adopted
            debug!("Received a Notion webhook verification request");
            current_span.set_attribute(attr::NOTION_EVENT_OUTCOME, "verification");
            return Ok(HttpResponse::Ok().finish());
        }
        current_span.set_attribute(attr::NOTION_EVENT_OUTCOME, "rejected");
        current_span.set_attribute(attr::NOTION_EVENT_REJECT_REASON, reason);
        warn!(reason, "Rejected unsigned/invalid Notion webhook");
        return Ok(HttpResponse::Unauthorized().finish());
    }

    let event: NotionWebhookEvent = serde_json::from_slice(&body)
        .context("Failed to deserialize Notion webhook event after signature verification")?;
    current_span.set_attribute(attr::NOTION_EVENT_ID, event.id.clone());
    current_span.set_attribute(attr::NOTION_EVENT_TYPE, event.event_type.as_str());
    current_span.set_attribute(attr::NOTION_WORKSPACE_ID, event.workspace_id.clone());
    if !event.event_type.is_handled_event() {
        current_span.set_attribute(attr::NOTION_EVENT_OUTCOME, "discarded");
        current_span.set_attribute(attr::NOTION_EVENT_DISCARD_REASON, "not_handled_event");
        return Ok(HttpResponse::Ok().finish());
    }

    let service = integration_connection_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while checking a Notion event")?;
    let has_workspace_connection = service
        .has_validated_integration_connection_in_provider_workspace(
            &mut transaction,
            IntegrationProviderKind::Notion,
            event.workspace_id.clone(),
        )
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit while checking a Notion event")?;
    if !has_workspace_connection {
        debug!("Ignoring a Notion event of a workspace without Universal Inbox user");
        current_span.set_attribute(attr::NOTION_EVENT_OUTCOME, "discarded");
        current_span.set_attribute(attr::NOTION_EVENT_DISCARD_REASON, "no_workspace_connection");
        return Ok(HttpResponse::Ok().finish());
    }

    let event_id = event.id.clone();
    let task_id = Retry::start(
        ExponentialBackoff::from_millis(10).map(jitter).take(10),
        || async {
            push_job(
                &storage,
                UniversalInboxJob::NotionWebhookEvent(NotionWebhookEventJob(event.clone())),
            )
            .await
        },
    )
    .await
    .context("Failed to push Notion event to queue")?;
    debug!("Pushed Notion event {event_id} to the queue with job ID {task_id}");
    current_span.set_attribute(attr::NOTION_EVENT_OUTCOME, "queued");

    Ok(HttpResponse::Ok().finish())
}

/// Verify a Notion webhook signature: `X-Notion-Signature` is `sha256=` and
/// the hex `HMAC-SHA256(verification_token, body)`. Notion sends no
/// timestamp, so there is no replay window; the job re-reads the comment from
/// Notion, so a replayed event changes nothing.
fn verify_notion_signature_parts(
    signature_header: Option<&str>,
    body: &[u8],
    signing_secret: &str,
) -> Result<(), &'static str> {
    if signing_secret.is_empty() {
        return Err("no signing secret configured");
    }
    let signature = signature_header.ok_or("missing signature header")?;
    let Some(provided_hex) = signature.strip_prefix("sha256=") else {
        return Err("missing sha256 prefix");
    };
    let provided_bytes = hex::decode(provided_hex).map_err(|_| "non-hex signature")?;

    let key = hmac::Key::new(hmac::HMAC_SHA256, signing_secret.as_bytes());
    let expected = hmac::sign(&key, body);

    if expected.as_ref().ct_eq(&provided_bytes).into() {
        Ok(())
    } else {
        Err("signature mismatch")
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub async fn push_slack_event(
    req: HttpRequest,
    body: web::Bytes,
    signing_secret: web::Data<Option<SlackSigningSecret>>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    third_party_item_service: web::Data<Arc<RwLock<ThirdPartyItemService>>>,
    storage: web::Data<JobStorage>,
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
            current_span.set_attribute(attr::SLACK_EVENT_TYPE, "url_verification");
            current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "url_verification");
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
            current_span.set_attribute(attr::SLACK_EVENT_TYPE, event_type);
            current_span.set_attribute(attr::SLACK_TEAM_ID, event.team_id.to_string());
            current_span.set_attribute(attr::SLACK_EVENT_ID, event.event_id.to_string());
            current_span.set_attribute(attr::SLACK_USER_ID, user.to_string());
            current_span.set_attribute(attr::SLACK_REACTION_NAME, reaction.to_string());

            let service = integration_connection_service.read().await;
            // No `.context()`: keep a pool timeout as `DatabaseUnavailable` (503).
            let mut transaction = service.begin().await?;

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
                    current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "queued");
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
                    current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "discarded");
                    current_span
                        .set_attribute(attr::SLACK_EVENT_DISCARD_REASON, "reaction_sync_disabled");
                }
                Some(IntegrationConnectionConfig::Slack(_)) => {
                    current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "discarded");
                    current_span
                        .set_attribute(attr::SLACK_EVENT_DISCARD_REASON, "reaction_name_mismatch");
                }
                _ => {
                    current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "discarded");
                    current_span.set_attribute(attr::SLACK_EVENT_DISCARD_REASON, "no_slack_config");
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
            current_span.set_attribute(attr::SLACK_EVENT_TYPE, "message");
            current_span.set_attribute(attr::SLACK_TEAM_ID, event.team_id.to_string());
            current_span.set_attribute(attr::SLACK_EVENT_ID, event.event_id.to_string());
            current_span.set_attribute(attr::SLACK_MESSAGE_TS, ts.to_string());
            if let Some(thread_ts) = thread_ts {
                current_span.set_attribute(attr::SLACK_MESSAGE_THREAD_TS, thread_ts.to_string());
            }
            if let Some(channel) = channel {
                current_span.set_attribute(attr::SLACK_CHANNEL_ID, channel.to_string());
            }
            if let Some(user) = user {
                current_span.set_attribute(attr::SLACK_USER_ID, user.to_string());
            }

            // Cheap, DB-free check first: this endpoint receives every message of every
            // channel the app is in, so most events must not cost a pool connection.
            if has_slack_references_in_message(content) {
                current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "queued");
                current_span.set_attribute(attr::SLACK_EVENT_QUEUE_REASON, "has_references");
                send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
                return Ok(HttpResponse::Ok().finish());
            }

            // Only thread replies can belong to a known thread
            let is_known_thread = match thread_ts {
                Some(thread_ts) => {
                    let service = third_party_item_service.read().await;
                    // A single read: use a plain connection (no BEGIN/ROLLBACK), returned to
                    // the pool before the Redis job push below. Pool exhaustion surfaces as
                    // `DatabaseUnavailable` (503) so Slack retries the event.
                    let mut connection = service.connect().await?;
                    service
                        .has_third_party_item_for_source_id(
                            &mut connection,
                            ThirdPartyItemKind::SlackThread,
                            &thread_ts.0,
                        )
                        .await?
                }
                None => false,
            };

            if is_known_thread {
                current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "queued");
                current_span.set_attribute(attr::SLACK_EVENT_QUEUE_REASON, "known_thread");
                send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
                return Ok(HttpResponse::Ok().finish());
            }

            current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "discarded");
            current_span.set_attribute(
                attr::SLACK_EVENT_DISCARD_REASON,
                "no_references_no_known_thread",
            );
        }
        SlackPushEvent::AppRateLimited(SlackAppRateLimitedEvent {
            team_id,
            minute_rate_limited,
            api_app_id,
        }) => {
            current_span.set_attribute(attr::SLACK_EVENT_TYPE, "app_rate_limited");
            current_span.set_attribute(attr::SLACK_TEAM_ID, team_id.to_string());
            current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "rate_limited");
            warn!(
                ?team_id,
                ?api_app_id,
                ?minute_rate_limited,
                "Slack pushed events are rate limited"
            );
        }
        SlackPushEvent::EventCallback(SlackPushEventCallback {
            ref team_id,
            ref event_id,
            ref event_time,
            event: SlackEventCallbackBody::AppUninstalled(_),
            ..
        }) => {
            current_span.set_attribute(attr::SLACK_EVENT_TYPE, "app_uninstalled");
            current_span.set_attribute(attr::SLACK_TEAM_ID, team_id.to_string());
            current_span.set_attribute(attr::SLACK_EVENT_ID, event_id.to_string());
            revoke_slack_access(
                integration_connection_service.as_ref(),
                team_id,
                None,
                event_time,
            )
            .await?;
        }
        SlackPushEvent::EventCallback(SlackPushEventCallback {
            ref team_id,
            ref event_id,
            ref event_time,
            event: SlackEventCallbackBody::Unknown(ref body),
            ..
        }) if body.get("type").and_then(|t| t.as_str()) == Some("tokens_revoked") => {
            current_span.set_attribute(attr::SLACK_EVENT_TYPE, "tokens_revoked");
            current_span.set_attribute(attr::SLACK_TEAM_ID, team_id.to_string());
            current_span.set_attribute(attr::SLACK_EVENT_ID, event_id.to_string());
            match serde_json::from_value::<SlackTokensRevokedEvent>(body.clone()) {
                Ok(SlackTokensRevokedEvent {
                    tokens: SlackRevokedTokens { oauth },
                }) if !oauth.is_empty() => {
                    revoke_slack_access(
                        integration_connection_service.as_ref(),
                        team_id,
                        Some(&oauth),
                        event_time,
                    )
                    .await?;
                }
                Ok(_) => {
                    current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "discarded");
                    current_span
                        .set_attribute(attr::SLACK_EVENT_DISCARD_REASON, "no_user_token_revoked");
                }
                Err(err) => {
                    current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "discarded");
                    current_span.set_attribute(attr::SLACK_EVENT_DISCARD_REASON, "invalid_payload");
                    warn!(
                        ?team_id,
                        ?event_id,
                        "Failed to parse Slack tokens_revoked event: {err}"
                    );
                }
            }
        }
        SlackPushEvent::EventCallback(SlackPushEventCallback {
            team_id,
            api_app_id,
            event_id,
            ..
        }) => {
            current_span.set_attribute(attr::SLACK_EVENT_TYPE, "unknown");
            current_span.set_attribute(attr::SLACK_TEAM_ID, team_id.to_string());
            current_span.set_attribute(attr::SLACK_EVENT_ID, event_id.to_string());
            current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "discarded");
            current_span.set_attribute(attr::SLACK_EVENT_DISCARD_REASON, "unknown_event_type");
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

/// `tokens_revoked` event payload, which slack-morphism does not model.
/// See <https://docs.slack.dev/reference/events/tokens_revoked>.
#[derive(Deserialize)]
struct SlackTokensRevokedEvent {
    tokens: SlackRevokedTokens,
}

#[derive(Deserialize)]
struct SlackRevokedTokens {
    /// Ids of the Slack users whose user token was revoked.
    #[serde(default)]
    oauth: Vec<String>,
}

async fn revoke_slack_access(
    integration_connection_service: &RwLock<IntegrationConnectionService>,
    team_id: &SlackTeamId,
    provider_user_ids: Option<&[String]>,
    revoked_at: &SlackDateTime,
) -> Result<(), UniversalInboxError> {
    let current_span = tracing::Span::current();
    let service = integration_connection_service.read().await;
    // No `.context()`: keep a pool timeout as `DatabaseUnavailable` (503).
    let mut transaction = service.begin().await?;
    let revoked = service
        .revoke_slack_access(
            &mut transaction,
            &team_id.0,
            provider_user_ids,
            revoked_at.0,
        )
        .await?;
    transaction
        .commit()
        .await
        .context("Failed to commit Slack access revocation")?;
    current_span.set_attribute(attr::SLACK_EVENT_OUTCOME, "access_revoked");
    current_span.set_attribute(attr::SLACK_REVOKED_CONNECTIONS_COUNT, revoked as i64);
    Ok(())
}

async fn send_slack_push_event_callback_job(
    storage: &JobStorage,
    event: SlackPushEventCallback,
) -> Result<(), UniversalInboxError> {
    let task_id = Retry::start(
        ExponentialBackoff::from_millis(10).map(jitter).take(10),
        || async {
            // The job, and the changes it makes, are attributed to the Slack webhook
            with_actor(
                AuditActor::Webhook {
                    provider: IntegrationProviderKind::Slack,
                },
                push_job(
                    storage,
                    UniversalInboxJob::SlackPushEventCallback(SlackPushEventCallbackJob(
                        event.clone(),
                    )),
                ),
            )
            .await
        },
    )
    .await
    .context("Failed to push Slack event to queue")?;
    debug!(
        "Pushed a Slack event {} to the queue with job ID {}",
        event.event_id, task_id
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
    // An empty secret is a key anyone can reproduce: `default.toml` ships
    // `signing_secret = ""`, and the boot guard only rejects it while the
    // Slack integration is enabled. Never verify against it.
    if signing_secret.is_empty() {
        return Err("no signing secret configured");
    }
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
    fn rejects_any_signature_when_the_secret_is_empty() {
        let body = b"{\"type\":\"event_callback\"}";
        let ts = NOW.to_string();
        let mut basestring = Vec::new();
        basestring.extend_from_slice(b"v0:");
        basestring.extend_from_slice(ts.as_bytes());
        basestring.push(b':');
        basestring.extend_from_slice(body);
        let empty_key = hmac::Key::new(hmac::HMAC_SHA256, b"");
        let forged = format!(
            "v0={}",
            hex::encode(hmac::sign(&empty_key, &basestring).as_ref())
        );
        assert_eq!(
            verify_slack_signature_parts(Some(&ts), Some(&forged), body, "", NOW),
            Err("no signing secret configured")
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

    fn sign_notion(body: &[u8]) -> String {
        let key = hmac::Key::new(hmac::HMAC_SHA256, SECRET.as_bytes());
        format!("sha256={}", hex::encode(hmac::sign(&key, body).as_ref()))
    }

    #[test]
    fn notion_signature_accepts_a_valid_signature() {
        let body = br#"{"type":"comment.created"}"#;
        assert_eq!(
            verify_notion_signature_parts(Some(&sign_notion(body)), body, SECRET),
            Ok(())
        );
    }

    #[test]
    fn notion_signature_rejects_a_tampered_body() {
        let signature = sign_notion(br#"{"type":"comment.created"}"#);
        assert_eq!(
            verify_notion_signature_parts(
                Some(&signature),
                br#"{"type":"comment.deleted"}"#,
                SECRET
            ),
            Err("signature mismatch")
        );
    }

    #[test]
    fn notion_signature_rejects_missing_or_malformed_headers() {
        let body = b"{}";
        assert_eq!(
            verify_notion_signature_parts(None, body, SECRET),
            Err("missing signature header")
        );
        assert_eq!(
            verify_notion_signature_parts(Some("abcd"), body, SECRET),
            Err("missing sha256 prefix")
        );
        assert_eq!(
            verify_notion_signature_parts(Some("sha256=zz"), body, SECRET),
            Err("non-hex signature")
        );
    }

    #[test]
    fn notion_signature_rejects_an_empty_secret() {
        let body = b"{}";
        let key = hmac::Key::new(hmac::HMAC_SHA256, b"");
        let signature = format!("sha256={}", hex::encode(hmac::sign(&key, body).as_ref()));
        assert_eq!(
            verify_notion_signature_parts(Some(&signature), body, ""),
            Err("no signing secret configured")
        );
    }
}
