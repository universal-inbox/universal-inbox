//! Span boundary helpers.
//!
//! Rules (see the tracing section of `.claude/docs/learnings/api-design.md`):
//! - INFO spans only at boundaries: HTTP root, job consumer, cron tick, CLI
//!   command, service entry points doing I/O, and external calls (client spans).
//!   Boundary spans declare `error.type = tracing::field::Empty` and end with
//!   [`RecordSpanError::record_span_error`] instead of `#[instrument(err)]`.
//! - DEBUG spans for internals, never with `err`: the boundary records the
//!   failure once.
//! - No span on per-item lookups, pure helpers and conversions.

use std::{fmt::Display, future::Future};

use tracing::{Instrument, Span, field::Empty};

use crate::{observability::attr, universal_inbox::UniversalInboxError};

/// `error.type` value when the error has no better class (OTel semconv).
pub const OTHER_ERROR_TYPE: &str = "_OTHER";

/// An error that can be recorded on a boundary span.
pub trait SpanError: Display {
    /// Low-cardinality class, recorded as `error.type`.
    fn error_type(&self) -> &'static str;
    /// The caller's fault (4xx): an expected outcome, logged at `warn` and not
    /// marking the span as Error.
    fn is_client_error(&self) -> bool;
}

impl SpanError for UniversalInboxError {
    fn error_type(&self) -> &'static str {
        UniversalInboxError::error_type(self)
    }

    fn is_client_error(&self) -> bool {
        UniversalInboxError::is_client_error(self)
    }
}

impl SpanError for anyhow::Error {
    fn error_type(&self) -> &'static str {
        self.downcast_ref::<UniversalInboxError>()
            .map(UniversalInboxError::error_type)
            .unwrap_or(OTHER_ERROR_TYPE)
    }

    fn is_client_error(&self) -> bool {
        self.downcast_ref::<UniversalInboxError>()
            .is_some_and(UniversalInboxError::is_client_error)
    }
}

/// Record a boundary result on the current span. On error, set `error.type`
/// and emit one event: `warn` for a client error (span status stays Unset),
/// `error` otherwise (tracing-opentelemetry then sets the span status to
/// Error). The span must declare `error.type = tracing::field::Empty`.
pub trait RecordSpanError {
    fn record_span_error(self) -> Self;
}

impl<T, E: SpanError> RecordSpanError for Result<T, E> {
    fn record_span_error(self) -> Self {
        if let Err(err) = &self {
            Span::current().record(attr::ERROR_TYPE, err.error_type());
            if err.is_client_error() {
                tracing::warn!(error = %format!("{err:#}"));
            } else {
                tracing::error!(error = %format!("{err:#}"));
            }
        }
        self
    }
}

/// An INFO client span for an outbound HTTP call not covered by the reqwest
/// tracing middleware. Named `<METHOD> <route>` like the reqwest client spans.
pub fn http_client_span(method: &'static str, server_address: &'static str, route: &str) -> Span {
    tracing::info_span!(
        "http client",
        { attr::OTEL_NAME } = %format!("{method} {route}"),
        { attr::OTEL_KIND } = "client",
        { attr::OTEL_STATUS_CODE } = Empty,
        { attr::HTTP_REQUEST_METHOD } = method,
        { attr::SERVER_ADDRESS } = server_address,
        { attr::URL_TEMPLATE } = route,
        { attr::ERROR_TYPE } = Empty,
    )
}

/// An INFO client span for a Redis command.
pub fn redis_client_span(operation: &'static str, namespace: &str) -> Span {
    tracing::info_span!(
        "redis client",
        { attr::OTEL_NAME } = %format!("{operation} {namespace}"),
        { attr::OTEL_KIND } = "client",
        { attr::OTEL_STATUS_CODE } = Empty,
        { attr::DB_SYSTEM_NAME } = "redis",
        { attr::DB_OPERATION_NAME } = operation,
        { attr::DB_NAMESPACE } = namespace,
        { attr::ERROR_TYPE } = Empty,
    )
}

/// Run `call` inside the client `span` and record a failure on it: status Error
/// plus `error.type` from `error_type`. No log event is emitted: the boundary
/// span owning the call logs the failure if it propagates.
pub async fn instrument_client_call<T, E, F>(
    span: Span,
    call: F,
    error_type: impl FnOnce(&E) -> String,
) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    let result = call.instrument(span.clone()).await;
    if let Err(err) = &result {
        span.record(attr::ERROR_TYPE, error_type(err));
        span.record(attr::OTEL_STATUS_CODE, "ERROR");
    }
    result
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use anyhow::anyhow;
    use pretty_assertions::assert_eq;
    use tracing::{Level, Subscriber, field::Visit, span};
    use tracing_subscriber::{Layer, Registry, layer::SubscriberExt, registry::LookupSpan};

    use super::*;

    #[derive(Clone, Default)]
    struct Recorded {
        events: Arc<Mutex<Vec<Level>>>,
        fields: Arc<Mutex<Vec<(String, String)>>>,
    }

    struct FieldVisitor<'a>(&'a mut Vec<(String, String)>);

    impl Visit for FieldVisitor<'_> {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.push((field.name().to_string(), value.to_string()));
        }

        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0
                .push((field.name().to_string(), format!("{value:?}")));
        }
    }

    impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Recorded {
        fn on_record(
            &self,
            _id: &span::Id,
            values: &span::Record<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            values.record(&mut FieldVisitor(&mut self.fields.lock().unwrap()));
        }

        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            self.events.lock().unwrap().push(*event.metadata().level());
        }
    }

    fn record_in_boundary_span<E: SpanError>(error: E) -> (Vec<Level>, Vec<(String, String)>) {
        let recorded = Recorded::default();
        let subscriber = Registry::default().with(recorded.clone());
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("boundary", { attr::ERROR_TYPE } = Empty);
            let _guard = span.enter();
            let _ = Err::<(), E>(error).record_span_error();
        });
        let events = recorded.events.lock().unwrap().clone();
        let fields = recorded.fields.lock().unwrap().clone();
        (events, fields)
    }

    fn error_type_field(value: &str) -> Vec<(String, String)> {
        vec![("error.type".to_string(), value.to_string())]
    }

    #[test]
    fn client_error_is_a_warning() {
        let (events, fields) =
            record_in_boundary_span(UniversalInboxError::Unauthorized(anyhow!("wrong password")));

        assert_eq!(events, vec![Level::WARN]);
        assert_eq!(fields, error_type_field("Unauthorized"));
    }

    #[test]
    fn server_error_is_an_error() {
        let (events, fields) =
            record_in_boundary_span(UniversalInboxError::Unexpected(anyhow!("boom")));

        assert_eq!(events, vec![Level::ERROR]);
        assert_eq!(fields, error_type_field("Unexpected"));
    }

    #[test]
    fn anyhow_error_wrapping_a_client_error_is_a_warning() {
        let (events, fields) = record_in_boundary_span(anyhow::Error::from(
            UniversalInboxError::ItemNotFound("task".to_string()),
        ));

        assert_eq!(events, vec![Level::WARN]);
        assert_eq!(fields, error_type_field("ItemNotFound"));
    }

    #[test]
    fn opaque_anyhow_error_is_an_error() {
        let (events, fields) = record_in_boundary_span(anyhow!("boom"));

        assert_eq!(events, vec![Level::ERROR]);
        assert_eq!(fields, error_type_field(OTHER_ERROR_TYPE));
    }

    #[test]
    fn ok_result_records_nothing() {
        let recorded = Recorded::default();
        let subscriber = Registry::default().with(recorded.clone());
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("boundary", { attr::ERROR_TYPE } = Empty);
            let _guard = span.enter();
            let _ = Ok::<(), UniversalInboxError>(()).record_span_error();
        });

        assert!(recorded.events.lock().unwrap().is_empty());
        assert!(recorded.fields.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_client_call_sets_error_status_without_logging() {
        let recorded = Recorded::default();
        let subscriber = Registry::default().with(recorded.clone());
        let _default = tracing::subscriber::set_default(subscriber);

        let result = instrument_client_call(
            http_client_span("POST", "slack.com", "/api/chat.getPermalink"),
            async { Err::<(), &str>("channel_not_found") },
            |err| err.to_string(),
        )
        .await;

        assert!(result.is_err());
        assert!(recorded.events.lock().unwrap().is_empty());
        assert_eq!(
            recorded.fields.lock().unwrap().clone(),
            vec![
                ("error.type".to_string(), "channel_not_found".to_string()),
                ("otel.status_code".to_string(), "ERROR".to_string()),
            ]
        );
    }
}
