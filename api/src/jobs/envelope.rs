//! Trace context propagation across the job queue.
//!
//! apalis-redis 0.7 only persists the job arguments, so the W3C trace context of
//! the producer travels inside the payload, in a [`JobEnvelope`]. Following the
//! OTel messaging semantic conventions, the consumer does not continue the
//! producer's trace: each job runs in a new trace whose root `process` span
//! carries a span link to the `send` span that enqueued it.

use std::collections::HashMap;

use apalis::{layers::tracing::MakeSpan, prelude::*};
use apalis_redis::{RedisError, RedisStorage};
use opentelemetry::{global, trace::TraceContextExt};
use serde::{Deserialize, Serialize};
use tracing::{Instrument, Span, field, info_span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{jobs::UniversalInboxJob, observability::attr};

pub type JobStorage = RedisStorage<JobEnvelope>;

/// Payload stored on the job queue: the job and the W3C trace context of the
/// span that enqueued it.
///
/// The job is flattened so a payload queued before trace propagation existed (a
/// bare `UniversalInboxJob`) still deserializes, with an empty context.
#[derive(Debug, Serialize, Deserialize)]
pub struct JobEnvelope {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub trace_context: HashMap<String, String>,
    #[serde(flatten)]
    pub job: UniversalInboxJob,
}

impl JobEnvelope {
    /// Wraps `job` with the trace context of the current span.
    pub fn new(job: UniversalInboxJob) -> Self {
        let mut trace_context = HashMap::new();
        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&Span::current().context(), &mut trace_context)
        });
        JobEnvelope { trace_context, job }
    }
}

/// Enqueues `job` within a `send` producer span whose context travels with the
/// job, so the job's `process` span links back to it.
pub async fn push_job(storage: &JobStorage, job: UniversalInboxJob) -> Result<TaskId, RedisError> {
    let job_name = job.name();
    let span = info_span!(
        "send",
        { attr::OTEL_NAME } = format!("send {job_name}"),
        { attr::OTEL_KIND } = "producer",
        { attr::MESSAGING_SYSTEM } = "redis",
        { attr::MESSAGING_DESTINATION_NAME } = storage.get_config().get_namespace(),
        { attr::MESSAGING_OPERATION_TYPE } = "send",
        { attr::MESSAGING_MESSAGE_ID } = field::Empty,
        { attr::JOB_NAME } = job_name,
    );
    let envelope = span.in_scope(|| JobEnvelope::new(job));
    let parts = storage
        .clone()
        .push(envelope)
        .instrument(span.clone())
        .await?;
    span.record(attr::MESSAGING_MESSAGE_ID, parts.task_id.to_string());
    Ok(parts.task_id)
}

/// Makes the root span of a job: a `process` consumer span in a new trace,
/// linked to the producer span whose context the job carries.
#[derive(Clone, Debug)]
pub struct JobMakeSpan {
    destination: String,
}

impl JobMakeSpan {
    pub fn new(storage: &JobStorage) -> Self {
        JobMakeSpan {
            destination: storage.get_config().get_namespace().to_string(),
        }
    }
}

impl<Ctx> MakeSpan<JobEnvelope, Ctx> for JobMakeSpan {
    fn make_span(&mut self, request: &Request<JobEnvelope, Ctx>) -> Span {
        let job_name = request.args.job.name();
        let span = info_span!(
            parent: None,
            "process",
            { attr::OTEL_NAME } = format!("process {job_name}"),
            { attr::OTEL_KIND } = "consumer",
            { attr::MESSAGING_SYSTEM } = "redis",
            { attr::MESSAGING_DESTINATION_NAME } = %self.destination,
            { attr::MESSAGING_OPERATION_TYPE } = "process",
            { attr::MESSAGING_MESSAGE_ID } = %request.parts.task_id,
            { attr::JOB_NAME } = job_name,
            { attr::JOB_ATTEMPT } = request.parts.attempt.current(),
            { attr::ERROR_TYPE } = field::Empty,
        );
        let producer_context = global::get_text_map_propagator(|propagator| {
            propagator.extract(&request.args.trace_context)
        });
        let producer_span_context = producer_context.span().span_context().clone();
        if producer_span_context.is_valid() {
            span.add_link(producer_span_context);
        }
        span
    }
}

#[cfg(test)]
mod tests {
    use opentelemetry::trace::{SpanKind, TracerProvider as _};
    use opentelemetry_sdk::{
        propagation::TraceContextPropagator,
        trace::{InMemorySpanExporter, SdkTracerProvider},
    };
    use pretty_assertions::assert_eq;
    use tracing_subscriber::{Registry, layer::SubscriberExt};

    use super::*;
    use crate::jobs::sync::SyncTasksJob;

    fn sync_tasks_job() -> UniversalInboxJob {
        UniversalInboxJob::SyncTasks(SyncTasksJob {
            source: None,
            user_id: None,
        })
    }

    #[test]
    fn test_payload_queued_before_trace_propagation_still_deserializes() {
        let legacy_payload = serde_json::to_string(&sync_tasks_job()).unwrap();

        let envelope: JobEnvelope = serde_json::from_str(&legacy_payload).unwrap();

        assert!(envelope.trace_context.is_empty());
        assert_eq!(envelope.job.name(), "SyncTasks");
    }

    #[test]
    fn test_envelope_round_trips_its_trace_context() {
        let envelope = JobEnvelope {
            trace_context: HashMap::from([(
                "traceparent".to_string(),
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".to_string(),
            )]),
            job: sync_tasks_job(),
        };

        let payload = serde_json::to_string(&envelope).unwrap();
        let decoded: JobEnvelope = serde_json::from_str(&payload).unwrap();

        assert_eq!(decoded.trace_context, envelope.trace_context);
        assert_eq!(decoded.job.name(), "SyncTasks");
    }

    #[test]
    fn test_job_span_is_a_new_trace_linked_to_the_producer_span() {
        global::set_text_map_propagator(TraceContextPropagator::new());
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber = Registry::default()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));

        tracing::subscriber::with_default(subscriber, || {
            let producer = info_span!("producer");
            let envelope = producer.in_scope(|| JobEnvelope::new(sync_tasks_job()));
            drop(producer);

            let request = Request::new_with_ctx(envelope, ());
            let consumer = JobMakeSpan {
                destination: "jobs".to_string(),
            }
            .make_span(&request);
            drop(consumer);
        });

        let spans = exporter.get_finished_spans().unwrap();
        let producer = spans.iter().find(|span| span.name == "producer").unwrap();
        let consumer = spans
            .iter()
            .find(|span| span.name == "process SyncTasks")
            .unwrap();
        assert_eq!(consumer.span_kind, SpanKind::Consumer);
        assert_eq!(
            consumer.parent_span_id,
            opentelemetry::trace::SpanId::INVALID
        );
        assert_ne!(
            consumer.span_context.trace_id(),
            producer.span_context.trace_id()
        );
        assert_eq!(consumer.links.links.len(), 1);
        let link = &consumer.links.links[0].span_context;
        assert_eq!(link.trace_id(), producer.span_context.trace_id());
        assert_eq!(link.span_id(), producer.span_context.span_id());
    }
}
