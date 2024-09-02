use std::collections::HashMap;
use std::{future::Future, str::FromStr, time::Duration};

use actix_http::body::MessageBody;
use actix_jwt_authc::Authenticated;
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    HttpMessage,
};
use opentelemetry::{trace::TracerProvider, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporterBuilder, SpanExporterBuilder, WithExportConfig};
use opentelemetry_sdk::{
    runtime,
    trace::{self, RandomIdGenerator, Sampler},
    Resource,
};
use tokio::task::JoinHandle;
use tonic::metadata::{AsciiMetadataKey, MetadataMap};
use tonic::transport::ClientTlsConfig;
use tracing::{subscriber::set_global_default, Instrument, Span, Subscriber};
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use tracing_log::LogTracer;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};

use universal_inbox::user::UserId;

use crate::{
    configuration::{OtlpExporterProtocol, TracingSettings},
    utils::jwt::Claims,
};

pub fn get_subscriber_with_telemetry(
    environment: &str,
    env_filter_str: &str,
    config: &TracingSettings,
    service_name: &str,
    version: Option<String>,
) -> impl Subscriber + Send + Sync {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter_str));

    let hostname = hostname::get().unwrap().into_string().unwrap();
    let mut resource = vec![
        KeyValue::new("service.name", service_name.to_string()),
        KeyValue::new("host.name", hostname.clone()),
        KeyValue::new("deployment.environment", environment.to_string()),
    ];
    if let Some(ref version) = version {
        resource.push(KeyValue::new("service.version", version.to_string()));
    }
    let tracer_provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(
            trace::Config::default()
                .with_sampler(Sampler::AlwaysOn)
                .with_id_generator(RandomIdGenerator::default())
                .with_max_events_per_span(256)
                .with_max_attributes_per_span(64)
                .with_resource(Resource::new(resource.clone())),
        )
        .with_exporter(build_exporter_builder::<SpanExporterBuilder>(
            config.otlp_exporter_protocol,
            config.otlp_exporter_endpoint.to_string(),
            config.otlp_exporter_headers.clone(),
        ))
        .install_batch(runtime::Tokio)
        .unwrap();
    let mut tracer_builder = tracer_provider.tracer_builder("universal-inbox");
    if let Some(version) = version {
        tracer_builder = tracer_builder.with_version(version);
    }
    let tracer = tracer_builder.build();
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let logger = opentelemetry_otlp::new_pipeline()
        .logging()
        .with_resource(Resource::new(resource))
        .with_exporter(build_exporter_builder::<LogExporterBuilder>(
            config.otlp_exporter_protocol,
            config.otlp_exporter_endpoint.to_string(),
            config.otlp_exporter_headers.clone(),
        ))
        .install_batch(runtime::Tokio)
        .unwrap();

    // The bridge currently has a bug as it does not add the span_id and trace_id to the log record
    // See https://github.com/open-telemetry/opentelemetry-rust/pull/1394
    let logging = OpenTelemetryTracingBridge::new(&logger);
    let fmt = tracing_subscriber::fmt::layer().pretty();

    Registry::default()
        .with(env_filter)
        .with(fmt)
        .with(telemetry)
        .with(logging)
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
        let authenticated_value = request.extensions().get::<Authenticated<Claims>>().cloned();
        match authenticated_value
            .and_then(|v| v.claims.sub.parse::<UserId>().ok())
            .map(|user_id| user_id.to_string())
        {
            Some(user_id) => {
                tracing_actix_web::root_span!(level = tracing::Level::INFO, request, user.id = %user_id)
            }
            // No user authenticated
            _ => {
                tracing_actix_web::root_span!(level = tracing::Level::INFO, request)
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

fn build_exporter_builder<B>(
    otlp_exporter_protocol: OtlpExporterProtocol,
    otlp_exporter_endpoint: String,
    otlp_exporter_headers: HashMap<String, String>,
) -> B
where
    B: std::convert::From<opentelemetry_otlp::HttpExporterBuilder>
        + std::convert::From<opentelemetry_otlp::TonicExporterBuilder>,
{
    if otlp_exporter_protocol == OtlpExporterProtocol::Http {
        let mut headers = HashMap::with_capacity(2);
        for (header_name, header_value) in &otlp_exporter_headers {
            if !header_value.is_empty() {
                headers.insert(
                    // header names usually use dashes instead of underscores but env vars don't allow dashes
                    header_name.replace('_', "-"),
                    header_value.parse().unwrap(),
                );
            }
        }

        opentelemetry_otlp::new_exporter()
            .http()
            .with_http_client(reqwest::Client::new())
            .with_endpoint(otlp_exporter_endpoint)
            .with_timeout(Duration::from_secs(3))
            .with_headers(headers.clone())
            .into()
    } else {
        let mut headers = MetadataMap::with_capacity(otlp_exporter_headers.len());
        for (header_name, header_value) in &otlp_exporter_headers {
            if !header_value.is_empty() {
                headers.insert(
                    // header names usually use dashes instead of underscores but env vars don't allow dashes
                    AsciiMetadataKey::from_str(header_name.replace('_', "-").as_str()).unwrap(),
                    header_value.parse().unwrap(),
                );
            }
        }

        opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(otlp_exporter_endpoint)
            .with_tls_config(ClientTlsConfig::new().with_native_roots())
            .with_timeout(Duration::from_secs(3))
            .with_metadata(headers.clone())
            .into()
    }
}
