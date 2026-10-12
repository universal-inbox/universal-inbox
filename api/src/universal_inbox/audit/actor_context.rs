//! Actor of the current task, recorded in the audit log by the database triggers.
//!
//! Entry points (HTTP middleware, MCP tool calls, job worker, CLI commands) run their work inside
//! [`with_actor`]. `Repository::begin` reads [`current_actor`] and stores it in the
//! transaction-local `app.audit_actor` setting read by the triggers.

use std::future::Future;

use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use universal_inbox::audit::AuditActor;

use crate::observability::attr;

tokio::task_local! {
    static AUDIT_ACTOR: AuditActor;
}

/// Run `future` with `actor` as the audit actor. A nested call replaces it for its own scope.
///
/// The actor is also recorded on the current span (`audit.actor.kind`, `audit.actor.job`).
pub async fn with_actor<F>(actor: AuditActor, future: F) -> F::Output
where
    F: Future,
{
    record_actor_on_span(&Span::current(), &actor);
    AUDIT_ACTOR.scope(actor, future).await
}

fn record_actor_on_span(span: &Span, actor: &AuditActor) {
    span.set_attribute(attr::AUDIT_ACTOR_KIND, actor.kind().to_string());
    if let AuditActor::System { job, .. } = actor {
        span.set_attribute(attr::AUDIT_ACTOR_JOB, job.clone());
    }
}

pub fn current_actor() -> Option<AuditActor> {
    AUDIT_ACTOR.try_with(Clone::clone).ok()
}

/// Run `future` as a system job, keeping the current actor as the one who triggered it.
pub async fn with_system_actor<F>(
    job: &str,
    source: Option<universal_inbox::integration_connection::provider::IntegrationProviderKind>,
    future: F,
) -> F::Output
where
    F: Future,
{
    let triggered_by = current_actor().and_then(|actor| match actor {
        // A job re-scoping itself (eg. a sync job calling the sync service) is not a trigger.
        AuditActor::System { triggered_by, .. } => triggered_by,
        actor => Some(Box::new(actor)),
    });
    with_actor(
        AuditActor::System {
            job: job.to_string(),
            source,
            triggered_by,
        },
        future,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use universal_inbox::{
        audit::AuditUserChannel, integration_connection::provider::IntegrationProviderKind,
    };

    #[tokio::test]
    async fn test_current_actor_is_scoped() {
        assert_eq!(current_actor(), None);

        let user = AuditActor::User {
            via: AuditUserChannel::Web,
        };
        let actor = with_actor(user.clone(), async { current_actor() }).await;

        assert_eq!(actor, Some(user));
        assert_eq!(current_actor(), None);
    }

    #[tokio::test]
    async fn test_actor_is_recorded_on_the_current_span() {
        use opentelemetry::{KeyValue, trace::TracerProvider as _};
        use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
        use tracing::Instrument;
        use tracing_subscriber::{Registry, layer::SubscriberExt};

        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber = Registry::default()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        let _guard = tracing::subscriber::set_default(subscriber);

        with_system_actor("SyncTasks", Some(IntegrationProviderKind::Todoist), async {
        })
        .instrument(tracing::info_span!("sync"))
        .await;

        let spans = exporter.get_finished_spans().unwrap();
        let sync = spans.iter().find(|span| span.name == "sync").unwrap();
        assert!(
            sync.attributes
                .contains(&KeyValue::new(attr::AUDIT_ACTOR_KIND, "System"))
        );
        assert!(
            sync.attributes
                .contains(&KeyValue::new(attr::AUDIT_ACTOR_JOB, "SyncTasks"))
        );
    }

    #[tokio::test]
    async fn test_system_actor_keeps_trigger() {
        let user = AuditActor::User {
            via: AuditUserChannel::Web,
        };

        let actor = with_actor(
            user.clone(),
            with_system_actor("sync-notifications", None, async {
                with_system_actor(
                    "sync-notifications",
                    Some(IntegrationProviderKind::Github),
                    async { current_actor() },
                )
                .await
            }),
        )
        .await;

        assert_eq!(
            actor,
            Some(AuditActor::System {
                job: "sync-notifications".to_string(),
                source: Some(IntegrationProviderKind::Github),
                triggered_by: Some(Box::new(user)),
            })
        );
    }
}
