use actix_web::{web, HttpResponse, Scope};
use anyhow::Context;
use apalis::{prelude::Storage, redis::RedisStorage};
use serde_json::json;
use slack_morphism::prelude::*;
use tracing::debug;

use crate::{
    jobs::{slack::SlackPushEventCallbackJob, UniversalInboxJob},
    universal_inbox::UniversalInboxError,
};

pub fn scope() -> Scope {
    web::scope("/hooks")
        .service(web::resource("/slack/events").route(web::post().to(push_slack_event)))
}

pub async fn push_slack_event(
    slack_push_event: web::Json<SlackPushEvent>,
    storage: web::Data<RedisStorage<UniversalInboxJob>>,
) -> Result<HttpResponse, UniversalInboxError> {
    match slack_push_event.into_inner() {
        SlackPushEvent::UrlVerification(SlackUrlVerificationEvent { challenge }) => {
            return Ok(HttpResponse::Ok()
                .content_type("application/json")
                .body(json!({ "challenge": challenge }).to_string()));
        }
        SlackPushEvent::EventCallback(event) => {
            let storage = &*storage.into_inner();
            let mut storage = storage.clone();
            let job_id = storage
                .push(UniversalInboxJob::SlackPushEventCallback(
                    SlackPushEventCallbackJob(event),
                ))
                .await
                .context("Failed to push Slack event to queue")?;
            debug!("Pushed a Slack event to the queue with job ID {job_id:?}");
        }
        event => {
            debug!("Received a push event from Slack: {event:?}");
        }
    }

    Ok(HttpResponse::Ok().finish())
}
