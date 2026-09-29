use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;
use std::{future::Future, time::Duration};

use crate::{
    configuration::{DEFAULT_TRUSTED_PROXY_HOPS, Settings},
    middlewares::jwt_auth::Authenticated,
    utils::rate_limit::{forwarded_for_chain, mask_forwarded_for},
};
use actix_http::body::MessageBody;
use actix_web::{
    HttpMessage,
    dev::{ServiceRequest, ServiceResponse},
    http::StatusCode,
    web,
};
use opentelemetry::{KeyValue, trace::TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::tonic_types::metadata::MetadataMap;
use opentelemetry_otlp::tonic_types::transport::ClientTlsConfig;
use opentelemetry_otlp::{
    LogExporter, SpanExporter, WithExportConfig, WithHttpConfig, WithTonicConfig,
};
use opentelemetry_sdk::{
    Resource,
    logs::{BatchLogProcessor, SdkLoggerProvider},
    trace::{BatchSpanProcessor, RandomIdGenerator, Sampler, SdkTracerProvider},
};
use tokio::task::JoinHandle;
use tonic::metadata::AsciiMetadataKey;
use tracing::{Instrument, Span, Subscriber, subscriber::set_global_default};
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use tracing_log::LogTracer;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    layer::{Layered, SubscriberExt},
    registry::LookupSpan,
};

use crate::{
    configuration::{LogFormat, OtlpExporterProtocol, TracingSettings},
    utils::jwt::Claims,
};

pub mod redaction;

use redaction::{RedactingLogProcessor, RedactingSpanProcessor};

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

/// Targets that must stay off whatever the operator configures.
///
/// `stripe_webhook`'s `parse_payload` is `#[tracing::instrument]` with the
/// webhook payload as a recorded argument, so enabling that target exports
/// whole Stripe event bodies — customer PII and invoice capability links — to
/// the trace backend. Forced off here rather than in `log_directive`, so
/// neither a config file nor `RUST_LOG` can turn it on.
const FORCED_OFF_TARGETS: [&str; 1] = ["stripe_webhook"];

/// Apply [`FORCED_OFF_TARGETS`] on top of an operator-supplied filter. A
/// directive added last wins over an earlier one for the same target, so this
/// overrides whatever the configuration said about those targets.
fn with_forced_off_targets(filter: EnvFilter) -> EnvFilter {
    FORCED_OFF_TARGETS.iter().fold(filter, |filter, target| {
        filter.add_directive(
            format!("{target}=off")
                .parse()
                .expect("a forced-off directive must parse"),
        )
    })
}

/// Kept so the panic hook can flush buffered spans and logs before the
/// process exits: batch processors would otherwise drop them.
static TELEMETRY_PROVIDERS: OnceLock<(SdkTracerProvider, SdkLoggerProvider)> = OnceLock::new();

fn build_env_filter(env_filter_str: &str) -> EnvFilter {
    with_forced_off_targets(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter_str)),
    )
}

pub fn get_subscriber_with_telemetry(
    environment: &str,
    env_filter_str: &str,
    config: &TracingSettings,
    service_name: &str,
    version: Option<String>,
) -> SubscriberWithTelemetry {
    let env_filter = build_env_filter(env_filter_str);

    let resource = build_resource(environment, service_name, version);
    let tracer_provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_max_events_per_span(256)
        .with_max_attributes_per_span(64)
        .with_resource(resource.clone())
        // Email addresses and client IPs are stripped before export (see
        // `redaction`): traces go to a third-party backend.
        .with_span_processor(RedactingSpanProcessor::new(
            BatchSpanProcessor::builder(build_span_exporter(
                config.otlp_exporter_protocol,
                config.otlp_exporter_endpoint.to_string(),
                config.otlp_exporter_headers.clone(),
            ))
            .build(),
        ))
        .build();
    let tracer = tracer_provider.tracer("universal-inbox");
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let logger = SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_log_processor(RedactingLogProcessor::new(
            BatchLogProcessor::builder(build_log_exporter(
                config.otlp_exporter_protocol,
                config.otlp_exporter_endpoint.to_string(),
                config.otlp_exporter_headers.clone(),
            ))
            .build(),
        ))
        .build();

    // The bridge currently has a bug as it does not add the span_id and trace_id to the log record
    // See https://github.com/open-telemetry/opentelemetry-rust/pull/1394
    let logging = OpenTelemetryTracingBridge::new(&logger);
    // Already set only if the subscriber is built twice: the first providers stay.
    let _ = TELEMETRY_PROVIDERS.set((tracer_provider, logger));

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
    log_format: LogFormat,
) -> impl Subscriber + Send + Sync {
    get_subscriber_with_telemetry(environment, env_filter_str, config, service_name, version)
        .with(build_fmt_layer(log_format))
}

pub fn get_subscriber(
    env_filter_str: &str,
    log_format: LogFormat,
) -> impl Subscriber + Send + Sync {
    let env_filter = build_env_filter(env_filter_str);

    Registry::default()
        .with(env_filter)
        .with(build_fmt_layer(log_format))
}

fn build_fmt_layer<S>(log_format: LogFormat) -> Box<dyn Layer<S> + Send + Sync>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    match log_format {
        LogFormat::Pretty => tracing_subscriber::fmt::layer().pretty().boxed(),
        // `flatten_event` puts `message` and `level` at the top level, where
        // log collectors such as Datadog read them without a custom pipeline.
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .flatten_event(true)
            .with_current_span(true)
            .boxed(),
    }
}

/// Report panics as `error` events so they reach the configured log output
/// and the OTLP backend, then run the previously installed hook (the colored
/// backtrace). Install it after the tracing subscriber.
pub fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");
        let location = panic_info
            .location()
            .map(ToString::to_string)
            .unwrap_or_default();
        tracing::error!(panic.location = %location, "The application panicked: {message}");

        if let Some((tracer_provider, logger_provider)) = TELEMETRY_PROVIDERS.get() {
            // Best effort: the process is going down and there is nowhere
            // left to report a failed export.
            let _ = tracer_provider.force_flush();
            let _ = logger_provider.force_flush();
        }

        previous_hook(panic_info);
    }));
}

pub fn init_subscriber(
    subscriber: impl Subscriber + Send + Sync,
    log_level_filter: log::LevelFilter,
) {
    LogTracer::init_with_filter(log_level_filter).expect("Failed to set logger");
    set_global_default(subscriber).expect("Failed to set subscriber");
}

pub struct AuthenticatedRootSpanBuilder;

/// Query parameters whose values are credentials (OAuth/OIDC authorization
/// codes and their CSRF state, tokens, PKCE verifiers) and must never reach
/// logs or exported traces.
const SENSITIVE_QUERY_PARAMETERS: &[&str] = &[
    "code",
    "state",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "code_verifier",
    "client_secret",
    "password",
];

/// Path segments that, in `/users/{user_id}/<segment>/<token>`, are followed
/// by a one-time secret (email-verification and password-reset tokens). The
/// shape is matched wherever it sits in the path, so both the API routes
/// (under the configurable API prefix) and the matching web app pages served
/// by the API are covered.
const ONE_TIME_TOKEN_PATH_SEGMENTS: &[&str] = &["email-verification", "password-reset"];

/// `path` with every one-time token segment (see
/// [`ONE_TIME_TOKEN_PATH_SEGMENTS`]) replaced by `REDACTED`.
fn redact_path(path: &str) -> String {
    let mut segments: Vec<&str> = path.split('/').collect();
    for index in 2..segments.len().saturating_sub(1) {
        if segments[index - 2] == "users"
            && ONE_TIME_TOKEN_PATH_SEGMENTS.contains(&segments[index])
            && !segments[index + 1].is_empty()
        {
            segments[index + 1] = "REDACTED";
        }
    }
    segments.join("/")
}

/// `path_and_query` with one-time token path segments (see [`redact_path`])
/// and the value of every [`SENSITIVE_QUERY_PARAMETERS`] entry replaced by
/// `REDACTED` (parameter names and other values are kept for debugging).
pub fn redact_path_and_query(path_and_query: &str) -> String {
    let Some((path, query)) = path_and_query.split_once('?') else {
        return redact_path(path_and_query);
    };
    let redacted_query = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _))
                if SENSITIVE_QUERY_PARAMETERS
                    .iter()
                    .any(|sensitive| key.eq_ignore_ascii_case(sensitive)) =>
            {
                format!("{key}=REDACTED")
            }
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{redacted_query}", redact_path(path))
}

/// A [`ServiceRequest`] view whose `uri()` has its credentials redacted.
///
/// `tracing_actix_web::root_span!` records `http.target` from
/// `request.uri().path_and_query()`, i.e. with the raw query string, which for
/// `/api/oauth/callback` and `/api/auth/session/authenticated` carries OAuth
/// authorization codes, and the raw path, which for email-verification and
/// password-reset links carries one-time tokens. Handing the macro this
/// wrapper (every other method derefs to the real request) keeps the default
/// span fields while recording a redacted target.
struct RedactedRequest<'a> {
    request: &'a ServiceRequest,
    uri: actix_web::http::Uri,
}

impl<'a> RedactedRequest<'a> {
    fn new(request: &'a ServiceRequest) -> Self {
        let original = request.uri();
        let uri = original
            .path_and_query()
            .and_then(|path_and_query| {
                actix_web::http::Uri::try_from(redact_path_and_query(path_and_query.as_str())).ok()
            })
            .unwrap_or_else(|| actix_web::http::Uri::from_static("/"));
        Self { request, uri }
    }

    fn uri(&self) -> &actix_web::http::Uri {
        &self.uri
    }
}

impl std::ops::Deref for RedactedRequest<'_> {
    type Target = ServiceRequest;

    fn deref(&self) -> &Self::Target {
        self.request
    }
}

/// Record the shape of the `X-Forwarded-For` chain on the root span, so an
/// operator can pick `application.security.trusted_proxy_hops` from traces:
/// on a request sent without its own `X-Forwarded-For`, the entry count is the
/// number of proxies appending to the header. Client addresses are masked
/// (see [`mask_forwarded_for`]); `http.client_ip` is dropped at export anyway.
fn record_forwarded_for(span: &Span, request: &ServiceRequest) {
    span.record(
        "rate_limit.trusted_proxy_hops",
        request
            .app_data::<web::Data<Settings>>()
            .map(|settings| settings.application.security.trusted_proxy_hops)
            .unwrap_or(DEFAULT_TRUSTED_PROXY_HOPS),
    );
    if let Some(chain) = forwarded_for_chain(request.headers()) {
        span.record("http.x_forwarded_for.entries", chain.split(',').count());
        span.record(
            "http.x_forwarded_for.masked",
            tracing::field::display(mask_forwarded_for(&chain)),
        );
    }
}

/// This is a custom root span builder that will add the user id to the root
/// span if the user is connected
impl RootSpanBuilder for AuthenticatedRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let authenticated_value = request.extensions().get::<Authenticated<Claims>>().cloned();
        let redacted_request = RedactedRequest::new(request);
        let request = &redacted_request;
        let span = match authenticated_value
            .and_then(|v| v.user_id_opt())
            .map(|user_id| user_id.to_string())
        {
            Some(user_id) => {
                tracing_actix_web::root_span!(
                    level = tracing::Level::INFO,
                    request,
                    user.id = %user_id,
                    http.x_forwarded_for.masked = tracing::field::Empty,
                    http.x_forwarded_for.entries = tracing::field::Empty,
                    rate_limit.trusted_proxy_hops = tracing::field::Empty,
                )
            }
            // No user authenticated
            _ => {
                tracing_actix_web::root_span!(
                    level = tracing::Level::INFO,
                    request,
                    http.x_forwarded_for.masked = tracing::field::Empty,
                    http.x_forwarded_for.entries = tracing::field::Empty,
                    rate_limit.trusted_proxy_hops = tracing::field::Empty,
                )
            }
        };
        record_forwarded_for(&span, request);
        span
    }

    fn on_request_end<B: MessageBody>(
        span: Span,
        outcome: &Result<ServiceResponse<B>, actix_web::Error>,
    ) {
        // A 401 is an expected outcome (logged-out clients), not an exception:
        // record the status only, so it does not surface as an error in traces
        let status = match outcome {
            Ok(response) => response.status(),
            Err(error) => error.as_response_error().status_code(),
        };
        if status == StatusCode::UNAUTHORIZED {
            span.record("http.status_code", i32::from(status.as_u16()));
            span.record("otel.status_code", "OK");
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn stripe_webhook_target_stays_off_even_when_configured_on() {
        // An operator directive naming the target — or a `RUST_LOG` carrying
        // one — must not bring the payload-bearing span back.
        let filter = with_forced_off_targets(EnvFilter::new("info,stripe_webhook=trace"));
        let directives: Vec<String> = filter.to_string().split(',').map(str::to_string).collect();

        assert!(
            directives.contains(&"stripe_webhook=off".to_string()),
            "expected the target to be forced off, got {directives:?}"
        );
        assert_eq!(
            directives
                .iter()
                .filter(|directive| directive.starts_with("stripe_webhook="))
                .count(),
            1,
            "the forced directive must replace the configured one, got {directives:?}"
        );
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::*;
    use rstest::*;

    #[rstest]
    #[case::no_query("/api/notifications", "/api/notifications")]
    #[case::harmless_query("/api/notifications?status=Unread", "/api/notifications?status=Unread")]
    #[case::oauth_callback(
        "/api/oauth/callback?code=ghu_secret&state=csrf123",
        "/api/oauth/callback?code=REDACTED&state=REDACTED"
    )]
    #[case::mixed(
        "/api/auth/session/authenticated?foo=bar&CODE=x&token=y",
        "/api/auth/session/authenticated?foo=bar&CODE=REDACTED&token=REDACTED"
    )]
    #[case::email_verification_token(
        "/api/users/0192f3a1-user/email-verification/secret-token",
        "/api/users/0192f3a1-user/email-verification/REDACTED"
    )]
    #[case::password_reset_token(
        "/api/users/0192f3a1-user/password-reset/secret-token",
        "/api/users/0192f3a1-user/password-reset/REDACTED"
    )]
    #[case::web_page_token(
        "/users/0192f3a1-user/email-verification/secret-token",
        "/users/0192f3a1-user/email-verification/REDACTED"
    )]
    #[case::token_and_query(
        "/api/users/0192f3a1-user/password-reset/secret-token?code=x&foo=bar",
        "/api/users/0192f3a1-user/password-reset/REDACTED?code=REDACTED&foo=bar"
    )]
    #[case::send_verification_email(
        "/api/users/me/email-verification",
        "/api/users/me/email-verification"
    )]
    #[case::send_password_reset_email("/api/users/password-reset", "/api/users/password-reset")]
    #[case::trailing_slash(
        "/api/users/0192f3a1-user/password-reset/",
        "/api/users/0192f3a1-user/password-reset/"
    )]
    #[case::other_user_route(
        "/api/users/me/authentication-tokens/0192f3a1-token-id",
        "/api/users/me/authentication-tokens/0192f3a1-token-id"
    )]
    fn test_redact_path_and_query(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(redact_path_and_query(input), expected);
    }
}
