use std::{future::Future, str::FromStr, time::Duration};

use actix_http::body::MessageBody;
use actix_identity::IdentityExt;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use anyhow::{anyhow, Context};
use log;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    runtime,
    trace::{self, RandomIdGenerator, Sampler},
    Resource,
};
use tokio::task::JoinHandle;
use tonic::metadata::{AsciiMetadataKey, MetadataMap};
use tracing::{subscriber::set_global_default, Instrument, Span, Subscriber};
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use tracing_log::LogTracer;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};

use universal_inbox::user::UserId;

use crate::configuration::TracingSettings;

pub fn get_subscriber_with_telemetry(
    environment: &str,
    env_filter_str: &str,
    config: &TracingSettings,
) -> impl Subscriber + Send + Sync {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter_str));

    let mut map = MetadataMap::with_capacity(config.otlp_exporter_headers.len());
    for (header_name, header_value) in config.otlp_exporter_headers.clone() {
        if !header_value.is_empty() {
            map.insert(
                // header names usually use dashes instead of underscores but env vars don't allow dashes
                AsciiMetadataKey::from_str(header_name.replace('_', "-").as_str()).unwrap(),
                header_value.parse().unwrap(),
            );
        }
    }

    let hostname = hostname::get().unwrap();
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(
            trace::config()
                .with_sampler(Sampler::AlwaysOn)
                .with_id_generator(RandomIdGenerator::default())
                .with_max_events_per_span(256)
                .with_max_attributes_per_span(64)
                .with_resource(Resource::new(vec![
                    KeyValue::new("service.name", "universal-inbox-api"),
                    KeyValue::new("hostname", hostname.into_string().unwrap()),
                    KeyValue::new("environment", environment.to_string()),
                ])),
        )
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(config.otlp_exporter_endpoint.to_string())
                .with_timeout(Duration::from_secs(3))
                .with_metadata(map),
        );
    let telemetry =
        tracing_opentelemetry::layer().with_tracer(tracer.install_batch(runtime::Tokio).unwrap());

    let fmt = tracing_subscriber::fmt::layer().compact();

    Registry::default()
        .with(env_filter)
        .with(fmt)
        .with(telemetry)
}

pub fn get_subscriber(env_filter_str: &str) -> impl Subscriber + Send + Sync {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter_str));
    let fmt = tracing_subscriber::fmt::layer().pretty();

    Registry::default().with(env_filter).with(fmt)
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
            .map_err(|err| anyhow!("Failed to fetch identity from request: {}", err))
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

pub fn spawn_blocking_with_tracing<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let current_span = tracing::Span::current();
    tokio::task::spawn_blocking(move || current_span.in_scope(f))
}

pub fn spawn_with_tracing<F>(f: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let current_span = tracing::Span::current();
    tokio::spawn(f.instrument(current_span))
}
