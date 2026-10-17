//! Optional Sentry error reporting, enabled by
//! `[application.observability.sentry]` (see [`SentrySettings`]).
//!
//! It composes with the OTLP and stdout layers: `ERROR` events become Sentry
//! events, `WARN` events breadcrumbs attached to the next event, and lower
//! levels (which may carry content) are never sent. Panics are reported by the
//! Sentry panic integration, with their stack trace. No performance traces:
//! the OTLP backend already has them.
//!
//! Same safety net as the OTLP export (see [`redaction`]): every string of an
//! event or breadcrumb has its email addresses and JWTs redacted before it is
//! sent, and [`FORCED_OFF_TARGETS`](super::FORCED_OFF_TARGETS) never reach
//! Sentry.

use secrecy::ExposeSecret;
use sentry::integrations::tracing::EventFilter;
use serde::{Serialize, de::DeserializeOwned};
use tracing::{Level, Metadata, Subscriber};
use tracing_subscriber::{EnvFilter, Layer, registry::LookupSpan};

use crate::configuration::SentrySettings;

use super::{redaction, with_forced_off_targets};

/// Target of the panic hook's `error!` event. Sentry ignores it: the panic
/// integration already reports the panic, with its stack trace. Under the
/// crate's target, so a `universal_inbox_api=...` directive still covers it.
pub const PANIC_TARGET: &str = "universal_inbox_api::panic";

/// Start the Sentry client, or do nothing when `settings` is `None`. The
/// returned guard flushes pending events when dropped: keep it alive for the
/// whole process.
///
/// Panics when the DSN is invalid, like other invalid settings.
pub fn init_sentry(
    settings: Option<&SentrySettings>,
    default_environment: &str,
) -> Option<sentry::ClientInitGuard> {
    let settings = settings?;
    let dsn = settings
        .dsn
        .expose_secret()
        .parse::<sentry::types::Dsn>()
        .expect("Failed to parse the Sentry DSN `application.observability.sentry.dsn`");
    let environment = settings
        .environment
        .clone()
        .unwrap_or_else(|| default_environment.to_string());

    let mut options = sentry::ClientOptions::new()
        .maybe_release(sentry::release_name!())
        .environment(environment)
        .sample_rate(settings.sample_rate)
        .send_default_pii(settings.send_default_pii)
        .before_send(redact_event)
        .before_breadcrumb(redact_breadcrumb);
    options.dsn = Some(dsn);
    Some(sentry::init(options))
}

/// The layer sending `tracing` events to Sentry (see [`event_filter`]). It
/// ignores `RUST_LOG` and the configured directives: only warnings and errors
/// are ever sent.
pub fn build_sentry_layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    sentry::integrations::tracing::layer()
        .event_filter(event_filter)
        // Spans would only become Sentry transactions, which are not sent.
        .span_filter(|_| false)
        .with_filter(sentry_filter())
}

/// Disables the lower levels before [`event_filter`] runs, which keeps the
/// layer cheap: keep its level in line with [`event_filter`]'s.
fn sentry_filter() -> EnvFilter {
    with_forced_off_targets(EnvFilter::new("warn"))
}

fn event_filter(metadata: &Metadata<'_>) -> EventFilter {
    if metadata.target() == PANIC_TARGET {
        return EventFilter::Ignore;
    }
    match *metadata.level() {
        Level::ERROR => EventFilter::Event,
        Level::WARN => EventFilter::Breadcrumb,
        _ => EventFilter::Ignore,
    }
}

fn redact_event(
    event: sentry::protocol::Event<'static>,
) -> Option<sentry::protocol::Event<'static>> {
    redact_all_strings(&event)
}

fn redact_breadcrumb(breadcrumb: sentry::Breadcrumb) -> Option<sentry::Breadcrumb> {
    redact_all_strings(&breadcrumb)
}

/// `value` with [`redaction::redact_json`] applied, through a JSON round trip.
/// `None` (the item is not sent) if the round trip fails: better lose an event
/// than send it unredacted.
fn redact_all_strings<T: Serialize + DeserializeOwned>(value: &T) -> Option<T> {
    let redacted = serde_json::to_value(value).and_then(|mut json| {
        redaction::redact_json(&mut json);
        serde_json::from_value(json)
    });
    match redacted {
        Ok(redacted) => Some(redacted),
        Err(error) => {
            // Not a tracing event: it would come back here as a breadcrumb.
            eprintln!("Dropped a Sentry item that could not be redacted: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use pretty_assertions::assert_eq;
    use tracing_subscriber::{Registry, layer::SubscriberExt};

    use super::*;

    const EMAIL: &str = "john.doe@example.com";
    const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl";

    #[test]
    fn test_init_sentry_without_settings_does_nothing() {
        assert!(init_sentry(None, "test").is_none());
        assert!(sentry::Hub::current().client().is_none());
    }

    #[test]
    fn test_redact_event_scrubs_message_exception_and_extra() {
        let leaky = format!("user {EMAIL} sent {JWT}");
        let event = sentry::protocol::Event {
            message: Some(leaky.clone()),
            exception: vec![sentry::protocol::Exception {
                ty: "Error".to_string(),
                value: Some(leaky.clone()),
                ..Default::default()
            }]
            .into(),
            extra: [("detail".to_string(), leaky.clone().into())].into(),
            ..Default::default()
        };

        let redacted = redact_event(event).expect("the event is kept");

        let sent = serde_json::to_string(&redacted).unwrap();
        assert!(!sent.contains(EMAIL), "{sent}");
        assert!(!sent.contains("eyJhbGciOiJIUzI1NiJ9"), "{sent}");
        let expected = format!(
            "user {} sent {}",
            redaction::REDACTED_EMAIL,
            redaction::REDACTED_JWT
        );
        assert_eq!(redacted.message.as_deref(), Some(expected.as_str()));
        assert_eq!(
            redacted.exception.values[0].value.as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(redacted.extra["detail"], expected);
    }

    /// Events built by the SDK itself (default integrations: contexts,
    /// stack traces, panic) and by the tracing layer, captured before
    /// `before_send`.
    fn sdk_events() -> Vec<sentry::protocol::Event<'static>> {
        let options = sentry::apply_defaults(
            sentry::ClientOptions::new()
                .release("universal-inbox-api@0.1.0")
                .environment("test"),
        );
        sentry::test::with_captured_events_options(
            || {
                let subscriber = Registry::default().with(build_sentry_layer());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::warn!(target: "universal_inbox_api", "a warning");
                    let error = std::io::Error::other("disk full");
                    tracing::error!(
                        target: "universal_inbox_api",
                        error = &error as &dyn std::error::Error,
                        "sync failed"
                    );
                });
                // The panic integration's hook reports to the current hub.
                let _ = std::panic::catch_unwind(|| panic!("boom"));
            },
            options,
        )
    }

    #[test]
    fn test_redact_event_round_trips_sdk_events_losslessly() {
        let events = sdk_events();

        assert_eq!(events.len(), 2, "{events:#?}");
        assert!(
            events[0]
                .breadcrumbs
                .iter()
                .any(|b| b.message.as_deref() == Some("a warning")),
            "{events:#?}"
        );
        assert!(
            events[0]
                .exception
                .values
                .iter()
                .any(|e| e.value.as_deref() == Some("disk full")),
            "{events:#?}"
        );
        let panic = events[1]
            .exception
            .values
            .iter()
            .find(|e| e.mechanism.as_ref().is_some_and(|m| m.ty == "panic"))
            .expect("a panic exception");
        assert!(panic.stacktrace.is_some(), "{events:#?}");
        for event in &events {
            assert!(event.contexts.contains_key("os"), "{event:#?}");
            assert_eq!(event.release.as_deref(), Some("universal-inbox-api@0.1.0"));
        }
        for event in events {
            let original = serde_json::to_value(&event).unwrap();
            let redacted = redact_event(event).expect("the event is kept");
            assert_eq!(serde_json::to_value(&redacted).unwrap(), original);
        }
    }

    #[test]
    fn test_redact_breadcrumb_scrubs_message() {
        let breadcrumb = sentry::Breadcrumb {
            message: Some(format!("login of {EMAIL}")),
            ..Default::default()
        };

        let redacted = redact_breadcrumb(breadcrumb).expect("the breadcrumb is kept");

        assert_eq!(
            redacted.message,
            Some(format!("login of {}", redaction::REDACTED_EMAIL))
        );
    }

    /// Records the `(target, level, filter bits)` of every event reaching it.
    #[derive(Clone, Default)]
    struct RecordedFilters(Arc<Mutex<Vec<(String, Level, u32)>>>);

    impl<S: Subscriber> Layer<S> for RecordedFilters {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let metadata = event.metadata();
            self.0.lock().unwrap().push((
                metadata.target().to_string(),
                *metadata.level(),
                event_filter(metadata).bits(),
            ));
        }
    }

    #[test]
    fn test_event_filter_sends_errors_and_records_warnings() {
        let recorded = RecordedFilters::default();
        let subscriber = Registry::default().with(recorded.clone());
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(target: "universal_inbox_api", "error");
            tracing::warn!(target: "universal_inbox_api", "warning");
            tracing::info!(target: "universal_inbox_api", "info");
            tracing::debug!(target: "universal_inbox_api", "debug");
            tracing::error!(target: PANIC_TARGET, "The application panicked");
        });

        assert_eq!(
            *recorded.0.lock().unwrap(),
            vec![
                (
                    "universal_inbox_api".to_string(),
                    Level::ERROR,
                    EventFilter::Event.bits()
                ),
                (
                    "universal_inbox_api".to_string(),
                    Level::WARN,
                    EventFilter::Breadcrumb.bits()
                ),
                (
                    "universal_inbox_api".to_string(),
                    Level::INFO,
                    EventFilter::Ignore.bits()
                ),
                (
                    "universal_inbox_api".to_string(),
                    Level::DEBUG,
                    EventFilter::Ignore.bits()
                ),
                (
                    PANIC_TARGET.to_string(),
                    Level::ERROR,
                    EventFilter::Ignore.bits()
                ),
            ]
        );
    }

    #[test]
    fn test_sentry_filter_drops_forced_off_targets_and_info() {
        let recorded = RecordedFilters::default();
        let subscriber = Registry::default().with(recorded.clone().with_filter(sentry_filter()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(target: "stripe_webhook", "payload");
            tracing::info!(target: "universal_inbox_api", "info");
            tracing::warn!(target: "universal_inbox_api", "warning");
        });

        assert_eq!(
            *recorded.0.lock().unwrap(),
            vec![(
                "universal_inbox_api".to_string(),
                Level::WARN,
                EventFilter::Breadcrumb.bits()
            )]
        );
    }
}
