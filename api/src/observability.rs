use std::collections::HashMap;
use std::{future::Future, str::FromStr, time::Duration};

use actix_http::body::MessageBody;
use actix_jwt_authc::Authenticated;
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    HttpMessage,
};
use opentelemetry::{trace::TracerProvider as _, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{
    LogExporter, SpanExporter, WithExportConfig, WithHttpConfig, WithTonicConfig,
};
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
    Resource,
};
use tokio::task::JoinHandle;
use tonic::metadata::{AsciiMetadataKey, MetadataMap};
use tonic::transport::ClientTlsConfig;
use tracing::{subscriber::set_global_default, Instrument, Span, Subscriber};
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use tracing_log::LogTracer;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{
    layer::{Layered, SubscriberExt},
    EnvFilter, Registry,
};

use universal_inbox::user::UserId;

use crate::{
    configuration::{OtlpExporterProtocol, TracingSettings},
    utils::jwt::Claims,
};

type SubscriberWithTelemetry = Layered<
    OpenTelemetryTracingBridge<
        opentelemetry_sdk::logs::SdkLoggerProvider,
        opentelemetry_sdk::logs::SdkLogger,
    >,
    Layered<
        OpenTelemetryLayer<Layered<EnvFilter, Registry>, opentelemetry_sdk::trace::Tracer>,
        Layered<EnvFilter, Registry>,
    >,
>;

pub fn get_subscriber_with_telemetry(
    environment: &str,
    env_filter_str: &str,
    config: &TracingSettings,
    service_name: &str,
    version: Option<String>,
) -> SubscriberWithTelemetry {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter_str));

    let resource = build_resource(environment, service_name, version);
    let tracer_provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_max_events_per_span(256)
        .with_max_attributes_per_span(64)
        .with_resource(resource.clone())
        .with_batch_exporter(build_span_exporter(
            config.otlp_exporter_protocol,
            config.otlp_exporter_endpoint.to_string(),
            config.otlp_exporter_headers.clone(),
        ))
        .build();
    let tracer = tracer_provider.tracer("universal-inbox");
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let logger = SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(build_log_exporter(
            config.otlp_exporter_protocol,
            config.otlp_exporter_endpoint.to_string(),
            config.otlp_exporter_headers.clone(),
        ))
        .build();

    // The bridge currently has a bug as it does not add the span_id and trace_id to the log record
    // See https://github.com/open-telemetry/opentelemetry-rust/pull/1394
    let logging = OpenTelemetryTracingBridge::new(&logger);

    Registry::default()
        .with(env_filter)
        .with(telemetry)
        .with(logging)
}

pub fn get_subscriber_with_telemetry_and_logging(
    environment: &str,
    env_filter_str: &str,
    config: &TracingSettings,
    service_name: &str,
    version: Option<String>,
) -> impl Subscriber + Send + Sync {
    let fmt = tracing_subscriber::fmt::layer().pretty();
    get_subscriber_with_telemetry(environment, env_filter_str, config, service_name, version)
        .with(fmt)
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

fn build_span_exporter(
    otlp_exporter_protocol: OtlpExporterProtocol,
    otlp_exporter_endpoint: String,
    otlp_exporter_headers: HashMap<String, String>,
) -> SpanExporter {
    let builder = SpanExporter::builder();

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

        builder
            .with_http()
            .with_http_client(reqwest::Client::new())
            .with_endpoint(otlp_exporter_endpoint)
            .with_timeout(Duration::from_secs(3))
            .with_headers(headers.clone())
            .build()
            .unwrap()
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

        builder
            .with_tonic()
            .with_endpoint(otlp_exporter_endpoint)
            .with_tls_config(ClientTlsConfig::new().with_native_roots())
            .with_timeout(Duration::from_secs(3))
            .with_metadata(headers.clone())
            .build()
            .unwrap()
    }
}

fn build_log_exporter(
    otlp_exporter_protocol: OtlpExporterProtocol,
    otlp_exporter_endpoint: String,
    otlp_exporter_headers: HashMap<String, String>,
) -> LogExporter
where
{
    let builder = LogExporter::builder();

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

        builder
            .with_http()
            .with_http_client(reqwest::Client::new())
            .with_endpoint(otlp_exporter_endpoint)
            .with_timeout(Duration::from_secs(3))
            .with_headers(headers.clone())
            .build()
            .unwrap()
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

        builder
            .with_tonic()
            .with_endpoint(otlp_exporter_endpoint)
            .with_tls_config(ClientTlsConfig::new().with_native_roots())
            .with_timeout(Duration::from_secs(3))
            .with_metadata(headers.clone())
            .build()
            .unwrap()
    }
}

fn build_resource(environment: &str, service_name: &str, version: Option<String>) -> Resource {
    let mut resource = vec![
        KeyValue::new("service.name", service_name.to_string()),
        KeyValue::new("deployment.environment", environment.to_string()),
    ];
    if let Some(ref version) = version {
        resource.push(KeyValue::new("service.version", version.to_string()));
    }

    Resource::builder()
        .with_service_name(service_name.to_string())
        .with_attributes(resource)
        .build()
}
