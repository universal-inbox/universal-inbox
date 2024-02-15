use std::sync::Arc;

use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use serde_json::json;
use slack_morphism::prelude::*;
use tokio::sync::RwLock;
use tracing::debug;

use crate::universal_inbox::{
    notification::{service::NotificationService, NotificationEventService},
    UniversalInboxError,
};

pub fn scope() -> Scope {
    web::scope("/hooks")
        .service(web::resource("/slack/events").route(web::post().to(push_slack_event)))
}

fn fail_if_needed<T>(
    result: Result<T, UniversalInboxError>,
) -> Result<Option<T>, UniversalInboxError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(UniversalInboxError::UnsupportedAction(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

pub async fn push_slack_event(
    slack_push_event: web::Json<SlackPushEvent>,
    notification_service: web::Data<Arc<RwLock<NotificationService>>>,
) -> Result<HttpResponse, UniversalInboxError> {
    match slack_push_event.into_inner() {
        SlackPushEvent::UrlVerification(SlackUrlVerificationEvent { challenge }) => {
            return Ok(HttpResponse::Ok()
                .content_type("application/json")
                .body(json!({ "challenge": challenge }).to_string()));
        }
        SlackPushEvent::EventCallback(event) => {
            let service = notification_service.read().await;
            let mut transaction = service
                .begin()
                .await
                .context("Failed to create new transaction while creating a Slack notification")?;

            fail_if_needed(
                service
                    .save_notification_from_event(&mut transaction, event)
                    .await,
            )?;

            transaction
                .commit()
                .await
                .context("Failed to commit while creating Slack notification")?;
        }
        event => {
            debug!("Received a push event from Slack: {event:?}");
        }
    }

    Ok(HttpResponse::Ok().finish())
}
