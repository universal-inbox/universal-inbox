//! Exporter-side safety net that strips email addresses from the telemetry
//! sent over OTLP (traces and logs are exported to a third party, Honeycomb,
//! in the US).
//!
//! Instrumented code must not record personal data in the first place (use
//! `user.id`, never the email / username / name). These processors only
//! catch what slips through: a span attribute, a span event (tracing events
//! recorded inside a span), a span status or a log body containing something
//! that looks like an email address has it replaced by [`REDACTED_EMAIL`]
//! before the batch processor exports it. The client IP address recorded by
//! the HTTP root span (`http.client_ip`) is dropped altogether.
//!
//! Limitation: log record *attributes* cannot be rewritten through the
//! OpenTelemetry SDK API, only the log body can. Structured log fields must
//! therefore never carry personal data.

use std::{borrow::Cow, sync::LazyLock, time::Duration};

use opentelemetry::{
    InstrumentationScope, KeyValue, Value,
    logs::{AnyValue, LogRecord},
    trace::Status,
};
use opentelemetry_sdk::{
    Resource,
    error::OTelSdkResult,
    logs::{LogProcessor, SdkLogRecord},
    trace::{Span, SpanData, SpanProcessor},
};
use regex::Regex;

pub const REDACTED_EMAIL: &str = "[redacted-email]";

/// Span attributes carrying personal data that are never exported.
const DROPPED_SPAN_ATTRIBUTES: &[&str] = &["http.client_ip", "client.address"];

static EMAIL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(\.[A-Za-z0-9\-]+)*\.[A-Za-z]{2,}")
        .expect("valid email pattern")
});

/// Replace every email address found in `text` by [`REDACTED_EMAIL`].
/// Borrows `text` untouched when it contains none (the common case).
pub fn redact_emails(text: &str) -> Cow<'_, str> {
    if !text.contains('@') {
        return Cow::Borrowed(text);
    }
    EMAIL_PATTERN.replace_all(text, REDACTED_EMAIL)
}

fn redact_owned(text: Cow<'static, str>) -> Cow<'static, str> {
    match redact_emails(&text) {
        Cow::Borrowed(_) => text,
        Cow::Owned(redacted) => Cow::Owned(redacted),
    }
}

fn redact_value(value: &mut Value) {
    if let Value::String(string) = value
        && let Cow::Owned(redacted) = redact_emails(string.as_str())
    {
        *value = Value::String(redacted.into());
    }
}

fn redact_attributes(attributes: &mut [KeyValue]) {
    for attribute in attributes {
        redact_value(&mut attribute.value);
    }
}

/// Redact email addresses from an ended span before it is exported.
pub fn redact_span(span: &mut SpanData) {
    span.attributes
        .retain(|attribute| !DROPPED_SPAN_ATTRIBUTES.contains(&attribute.key.as_str()));
    redact_attributes(&mut span.attributes);
    for event in span.events.events.iter_mut() {
        event.name = redact_owned(std::mem::take(&mut event.name));
        redact_attributes(&mut event.attributes);
    }
    if let Status::Error { description } = &mut span.status {
        *description = redact_owned(std::mem::take(description));
    }
}

/// Wraps a span processor (the OTLP batch processor) and redacts every span
/// before handing it over.
#[derive(Debug)]
pub struct RedactingSpanProcessor<P> {
    inner: P,
}

impl<P> RedactingSpanProcessor<P> {
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

impl<P: SpanProcessor> SpanProcessor for RedactingSpanProcessor<P> {
    fn on_start(&self, span: &mut Span, cx: &opentelemetry::Context) {
        self.inner.on_start(span, cx);
    }

    fn on_end(&self, mut span: SpanData) {
        redact_span(&mut span);
        self.inner.on_end(span);
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

/// Wraps a log processor (the OTLP batch processor) and redacts email
/// addresses from each log body before handing the record over.
#[derive(Debug)]
pub struct RedactingLogProcessor<P> {
    inner: P,
}

impl<P> RedactingLogProcessor<P> {
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

fn redacted_body(body: &AnyValue) -> Option<AnyValue> {
    match body {
        AnyValue::String(string) => match redact_emails(string.as_str()) {
            Cow::Owned(redacted) => Some(AnyValue::String(redacted.into())),
            Cow::Borrowed(_) => None,
        },
        _ => None,
    }
}

impl<P: LogProcessor> LogProcessor for RedactingLogProcessor<P> {
    fn emit(&self, data: &mut SdkLogRecord, instrumentation: &InstrumentationScope) {
        if let Some(body) = data.body().and_then(redacted_body) {
            data.set_body(body);
        }
        self.inner.emit(data, instrumentation);
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn event_enabled(
        &self,
        level: opentelemetry::logs::Severity,
        target: &str,
        name: Option<&str>,
    ) -> bool {
        self.inner.event_enabled(level, target, name)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use opentelemetry::trace::{SpanContext, SpanId, SpanKind};
    use opentelemetry_sdk::trace::{SpanEvents, SpanLinks};
    use pretty_assertions::assert_eq;

    #[test]
    fn redacts_emails_and_keeps_the_rest() {
        assert_eq!(
            redact_emails("No user found for email address john.doe+ui@example.co.uk, sorry"),
            "No user found for email address [redacted-email], sorry"
        );
        assert_eq!(
            redact_emails("a@b.io and c.d@e-f.com"),
            "[redacted-email] and [redacted-email]"
        );
    }

    #[test]
    fn leaves_text_without_email_borrowed() {
        assert!(matches!(
            redact_emails("user 42 @ step 3, see https://x.test/@handle"),
            Cow::Borrowed(_)
        ));
        assert!(matches!(redact_emails("no at sign"), Cow::Borrowed(_)));
    }

    fn span_with_personal_data() -> SpanData {
        let mut span = SpanData {
            span_context: SpanContext::empty_context(),
            parent_span_id: SpanId::INVALID,
            parent_span_is_remote: false,
            span_kind: SpanKind::Internal,
            name: "get_user_by_email".into(),
            start_time: std::time::SystemTime::now(),
            end_time: std::time::SystemTime::now(),
            attributes: vec![
                KeyValue::new("user.id", "0f7f7a5e"),
                KeyValue::new("user.email", "john@doe.name"),
                KeyValue::new("http.client_ip", "203.0.113.7"),
                KeyValue::new("count", 3),
            ],
            dropped_attributes_count: 0,
            events: SpanEvents::default(),
            links: SpanLinks::default(),
            status: Status::error("No user found for email address john@doe.name"),
            instrumentation_scope: InstrumentationScope::builder("test").build(),
        };
        span.events.events.push(opentelemetry::trace::Event::new(
            "Sending email to john@doe.name",
            std::time::SystemTime::now(),
            vec![KeyValue::new("to", "jane@doe.name")],
            0,
        ));

        span
    }

    #[test]
    fn redacts_span_attributes_events_and_status() {
        let mut span = span_with_personal_data();

        redact_span(&mut span);

        assert_eq!(
            span.attributes,
            vec![
                KeyValue::new("user.id", "0f7f7a5e"),
                KeyValue::new("user.email", REDACTED_EMAIL),
                KeyValue::new("count", 3),
            ]
        );
        let event = &span.events.events[0];
        assert_eq!(event.name, "Sending email to [redacted-email]");
        assert_eq!(event.attributes, vec![KeyValue::new("to", REDACTED_EMAIL)]);
        assert_eq!(
            span.status,
            Status::error("No user found for email address [redacted-email]")
        );
    }

    #[derive(Debug, Default)]
    struct CapturingProcessor {
        spans: std::sync::Mutex<Vec<SpanData>>,
    }

    impl SpanProcessor for CapturingProcessor {
        fn on_start(&self, _span: &mut Span, _cx: &opentelemetry::Context) {}
        fn on_end(&self, span: SpanData) {
            self.spans.lock().unwrap().push(span);
        }
        fn force_flush(&self) -> OTelSdkResult {
            Ok(())
        }
        fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
            Ok(())
        }
    }

    #[test]
    fn the_span_processor_forwards_redacted_spans() {
        let processor = RedactingSpanProcessor::new(CapturingProcessor::default());

        processor.on_end(span_with_personal_data());

        let spans = processor.inner.spans.lock().unwrap();
        assert_eq!(spans.len(), 1);
        assert!(
            spans[0]
                .attributes
                .iter()
                .all(|attribute| !attribute.value.as_str().contains('@')
                    && attribute.key.as_str() != "http.client_ip"),
            "{:?}",
            spans[0].attributes
        );
        assert!(!spans[0].events.events[0].name.contains('@'));
    }
}
