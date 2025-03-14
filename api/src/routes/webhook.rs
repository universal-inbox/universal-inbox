use std::sync::Arc;

use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use apalis::prelude::Storage;
use apalis_redis::RedisStorage;
use serde_json::json;
use slack_morphism::prelude::*;
use tokio::sync::RwLock;
use tokio_retry::{
    strategy::{jitter, ExponentialBackoff},
    Retry,
};
use tracing::{debug, warn};
use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackReactionConfig, SlackStarConfig},
        provider::IntegrationProviderKind,
    },
    third_party::item::ThirdPartyItemKind,
};

use crate::{
    integrations::slack::has_slack_references_in_message,
    jobs::{slack::SlackPushEventCallbackJob, UniversalInboxJob},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        third_party::service::ThirdPartyItemService, UniversalInboxError,
    },
};

pub fn scope() -> Scope {
    web::scope("/hooks")
        .service(web::resource("/slack/events").route(web::post().to(push_slack_event)))
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
            {
                if reaction_name == *reaction {
                    send_slack_push_event_callback_job(storage.as_ref(), event.clone()).await?;
                }
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
            if let Some(thread_ts) = &thread_ts {
                if service
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
