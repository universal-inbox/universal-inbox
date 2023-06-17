use actix_http::body::MessageBody;
use actix_identity::IdentityExt;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use anyhow::Context;
use log;
use tracing::{subscriber::set_global_default, Span, Subscriber};
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::{fmt::TestWriter, layer::SubscriberExt, EnvFilter};

use universal_inbox::user::UserId;

pub fn get_subscriber(env_filter_str: &str) -> impl Subscriber + Send + Sync {
    let formatting_layer =
        BunyanFormattingLayer::new("universal-inbox-api".into(), TestWriter::new);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter_str));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
}

pub fn init_subscriber(
    subscriber: impl Subscriber + Send + Sync,
    log_level_filter: log::LevelFilter,
) {
    LogTracer::init_with_filter(log_level_filter).expect("Failed to set logger");
    set_global_default(subscriber).expect("Failed to set subscriber");
}

pub struct AuthenticatedRootSpanBuilder;

/// This is a custom root span builder that will add the user id to the root
/// span if the user is connected
impl RootSpanBuilder for AuthenticatedRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let identity = request.get_identity();
        match identity
            .and_then(|id| id.id())
            .and_then(|id| {
                id.parse::<UserId>()
                    .context("Unable to parse user ID from {id}")
            })
            .map(|user_id| user_id.to_string())
        {
            Ok(session_user_id) => {
                tracing_actix_web::root_span!(request, session_user_id)
            }
            // No user authenticated
            Err(_) => {
                tracing_actix_web::root_span!(request)
            }
        }
    }

    fn on_request_end<B: MessageBody>(
        span: Span,
        outcome: &Result<ServiceResponse<B>, actix_web::Error>,
    ) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}
