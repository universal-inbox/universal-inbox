//! Metrics of the job queue: how long jobs wait and run, how many fail or retry, and how deep
//! the queue is. The queue gauges are sampled by every worker pod, aggregate them with `max`.

use std::{sync::OnceLock, time::Duration};

use apalis::prelude::BackendExpose;
use apalis_redis::RedisError;
use chrono::{DateTime, Utc};
use opentelemetry::{
    KeyValue,
    metrics::{Counter, Gauge, Histogram, Meter},
};
use tracing::warn;

use crate::{jobs::envelope::JobStorage, observability::attr};

/// How often the queue gauges are sampled
pub const QUEUE_SAMPLING_INTERVAL: Duration = Duration::from_secs(30);

/// Outcome of a job execution, as tagged on the metrics
const OUTCOME_SUCCESS: &str = "success";
const OUTCOME_FAILURE: &str = "failure";

pub struct JobMetrics {
    wait_duration: Histogram<f64>,
    execution_duration: Histogram<f64>,
    executions: Counter<u64>,
    retries: Counter<u64>,
    queue_length: Gauge<i64>,
    oldest_pending_age: Gauge<f64>,
}

impl JobMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            wait_duration: meter
                .f64_histogram("job.wait.duration")
                .with_unit("s")
                .with_description("Time a job waited in the queue before its first execution")
                .build(),
            execution_duration: meter
                .f64_histogram("job.execution.duration")
                .with_unit("s")
                .with_description("Time a job execution took")
                .build(),
            executions: meter
                .u64_counter("job.executions")
                .with_description("Job executions, by outcome")
                .build(),
            retries: meter
                .u64_counter("job.retries")
                .with_description("Job executions that retry a failed one")
                .build(),
            queue_length: meter
                .i64_gauge("job.queue.length")
                .with_description("Jobs in the queue, by state")
                .build(),
            oldest_pending_age: meter
                .f64_gauge("job.queue.oldest_pending.age")
                .with_unit("s")
                .with_description("Time the oldest pending job has been waiting")
                .build(),
        }
    }

    /// Records the start of an execution. The wait time is only measured on the first attempt:
    /// a retry would count the time the previous attempts took.
    pub fn job_started(
        &self,
        job_name: &'static str,
        attempt: usize,
        enqueued_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) {
        let attributes = [KeyValue::new(attr::JOB_NAME, job_name)];
        if attempt > 1 {
            self.retries.add(1, &attributes);
        } else if let Some(enqueued_at) = enqueued_at {
            self.wait_duration
                .record(age_seconds(enqueued_at, now), &attributes);
        }
    }

    /// Records the end of an execution, `error_type` being set when it failed.
    pub fn job_finished(
        &self,
        job_name: &'static str,
        duration: Duration,
        error_type: Option<&str>,
    ) {
        let outcome = if error_type.is_some() {
            OUTCOME_FAILURE
        } else {
            OUTCOME_SUCCESS
        };
        let mut attributes = vec![
            KeyValue::new(attr::JOB_NAME, job_name),
            KeyValue::new(attr::JOB_OUTCOME, outcome),
        ];
        self.execution_duration
            .record(duration.as_secs_f64(), &attributes);
        if let Some(error_type) = error_type {
            attributes.push(KeyValue::new(attr::ERROR_TYPE, error_type.to_string()));
        }
        self.executions.add(1, &attributes);
    }

    pub fn queue_sampled(&self, queue: &str, sample: &QueueSample) {
        for (state, length) in [
            ("pending", sample.pending),
            ("running", sample.running),
            ("failed", sample.failed),
            ("dead", sample.dead),
        ] {
            self.queue_length.record(
                length,
                &[
                    KeyValue::new(attr::MESSAGING_DESTINATION_NAME, queue.to_string()),
                    KeyValue::new(attr::JOB_QUEUE_STATE, state),
                ],
            );
        }
        self.oldest_pending_age.record(
            sample.oldest_pending_age,
            &[KeyValue::new(
                attr::MESSAGING_DESTINATION_NAME,
                queue.to_string(),
            )],
        );
    }
}

/// The job metrics of the process, recorded with the global meter provider set up by
/// `observability` (no-ops without it). The instruments are created on the first call: it
/// must come after the subscriber setup, which sets the meter provider.
pub fn job_metrics() -> &'static JobMetrics {
    static JOB_METRICS: OnceLock<JobMetrics> = OnceLock::new();
    JOB_METRICS.get_or_init(|| JobMetrics::new(&opentelemetry::global::meter("universal-inbox")))
}

/// Seconds elapsed from `since` to `now`, 0 if `since` is in the future (clock skew between pods).
fn age_seconds(since: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    (now - since).to_std().map_or(0.0, |age| age.as_secs_f64())
}

/// State of the job queue at a given time
#[derive(Debug, Clone, PartialEq)]
pub struct QueueSample {
    pub pending: i64,
    pub running: i64,
    pub failed: i64,
    pub dead: i64,
    /// Time the oldest pending job has been waiting, 0 if none or if it has no enqueue time
    pub oldest_pending_age: f64,
}

/// Reads the state of the queue of `storage`.
pub async fn sample_queue(
    storage: &JobStorage,
    now: DateTime<Utc>,
) -> Result<QueueSample, RedisError> {
    let stat = storage.stats().await?;
    let oldest_pending_age = oldest_pending_enqueued_at(storage)
        .await?
        .map_or(0.0, |enqueued_at| age_seconds(enqueued_at, now));
    Ok(QueueSample {
        pending: stat.pending as i64,
        running: stat.running as i64,
        failed: stat.failed as i64,
        dead: stat.dead as i64,
        oldest_pending_age,
    })
}

/// Enqueue time of the oldest pending job: jobs are pushed at the tail of the active list and
/// fetched from its head. Read directly rather than with `list_jobs`, which decodes ten jobs
/// and panics on a payload it cannot decode.
async fn oldest_pending_enqueued_at(
    storage: &JobStorage,
) -> Result<Option<DateTime<Utc>>, RedisError> {
    let mut conn = storage.get_connection().clone();
    let config = storage.get_config();
    let ids: Vec<String> = redis_apalis::cmd("LRANGE")
        .arg(config.active_jobs_list())
        .arg(0)
        .arg(0)
        .query_async(&mut conn)
        .await?;
    let Some(id) = ids.first() else {
        return Ok(None);
    };
    let payload: Option<Vec<u8>> = redis_apalis::cmd("HGET")
        .arg(config.job_data_hash())
        .arg(id)
        .query_async(&mut conn)
        .await?;
    Ok(payload.and_then(|payload| parse_enqueued_at(&payload)))
}

/// The enqueue time of a stored request (`{"args": <JobEnvelope>, "parts": ...}`).
fn parse_enqueued_at(payload: &[u8]) -> Option<DateTime<Utc>> {
    let request: serde_json::Value = serde_json::from_slice(payload).ok()?;
    serde_json::from_value(request.pointer("/args/enqueued_at")?.clone()).ok()
}

/// Samples the queue of `storage` every [`QUEUE_SAMPLING_INTERVAL`], for as long as it runs.
/// An error is logged and the next tick tries again.
pub async fn run_queue_sampler(storage: JobStorage) {
    let queue = storage.get_config().get_namespace().to_string();
    let mut interval = tokio::time::interval(QUEUE_SAMPLING_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        match sample_queue(&storage, Utc::now()).await {
            Ok(sample) => job_metrics().queue_sampled(&queue, &sample),
            Err(err) => warn!("Failed to sample the job queue: {err}"),
        }
    }
}

/// Metrics recorded in memory, for the tests of the code recording them.
#[cfg(test)]
mod test_support {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{
        InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
        data::{AggregatedMetrics, MetricData},
    };

    use super::*;

    pub(super) type Attributes = Vec<(String, String)>;

    pub(super) struct TestMetrics {
        exporter: InMemoryMetricExporter,
        provider: SdkMeterProvider,
        pub(super) metrics: JobMetrics,
    }

    impl TestMetrics {
        pub(super) fn new() -> Self {
            let exporter = InMemoryMetricExporter::default();
            let provider = SdkMeterProvider::builder()
                .with_reader(PeriodicReader::builder(exporter.clone()).build())
                .build();
            let metrics = JobMetrics::new(&provider.meter("test"));
            Self {
                exporter,
                provider,
                metrics,
            }
        }

        /// The data points (sorted attributes, value) of the metric `name` recorded so far: the
        /// total of a counter, the sum of a histogram, the last value of a gauge.
        pub(super) fn values(&self, name: &str) -> Vec<(Attributes, f64)> {
            self.provider.force_flush().unwrap();
            let mut points = vec![];
            // The exporter keeps every export: the last one holds the current values
            let finished_metrics = self.exporter.get_finished_metrics().unwrap();
            if let Some(resource_metrics) = finished_metrics.last() {
                for scope_metrics in resource_metrics.scope_metrics() {
                    for metric in scope_metrics
                        .metrics()
                        .filter(|metric| metric.name() == name)
                    {
                        match metric.data() {
                            AggregatedMetrics::U64(MetricData::Sum(sum)) => points.extend(
                                sum.data_points()
                                    .map(|p| (attributes(p.attributes()), p.value() as f64)),
                            ),
                            AggregatedMetrics::I64(MetricData::Gauge(gauge)) => points.extend(
                                gauge
                                    .data_points()
                                    .map(|p| (attributes(p.attributes()), p.value() as f64)),
                            ),
                            AggregatedMetrics::F64(MetricData::Gauge(gauge)) => points.extend(
                                gauge
                                    .data_points()
                                    .map(|p| (attributes(p.attributes()), p.value())),
                            ),
                            AggregatedMetrics::F64(MetricData::Histogram(histogram)) => points
                                .extend(
                                    histogram
                                        .data_points()
                                        .map(|p| (attributes(p.attributes()), p.sum())),
                                ),
                            data => panic!("Unexpected data for {name}: {data:?}"),
                        }
                    }
                }
            }
            points.sort_by(|a, b| a.0.cmp(&b.0));
            points
        }
    }

    impl Drop for TestMetrics {
        fn drop(&mut self) {
            let _ = self.provider.shutdown();
        }
    }

    fn attributes<'a>(attributes: impl Iterator<Item = &'a KeyValue>) -> Attributes {
        let mut attributes: Attributes = attributes
            .map(|kv| (kv.key.to_string(), kv.value.to_string()))
            .collect();
        attributes.sort();
        attributes
    }

    pub(super) fn attrs(pairs: &[(&str, &str)]) -> Attributes {
        let mut attributes: Attributes = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        attributes.sort();
        attributes
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;
    use pretty_assertions::assert_eq;

    use super::{
        test_support::{TestMetrics, attrs},
        *,
    };

    #[test]
    fn test_age_seconds_of_a_future_timestamp_is_zero() {
        let now = Utc::now();

        assert_eq!(age_seconds(now - TimeDelta::seconds(30), now), 30.0);
        assert_eq!(age_seconds(now + TimeDelta::seconds(5), now), 0.0);
    }

    #[test]
    fn test_enqueued_at_is_read_from_a_stored_request() {
        let payload =
            br#"{"args":{"enqueued_at":"2026-10-30T16:00:00Z","SyncTasks":{}},"parts":{}}"#;
        let legacy_payload = br#"{"args":{"SyncTasks":{}},"parts":{}}"#;

        assert_eq!(
            parse_enqueued_at(payload),
            Some("2026-10-30T16:00:00Z".parse().unwrap())
        );
        assert_eq!(parse_enqueued_at(legacy_payload), None);
        assert_eq!(parse_enqueued_at(b"not json"), None);
    }

    #[test]
    fn test_wait_time_is_recorded_on_the_first_attempt_only() {
        let test_metrics = TestMetrics::new();
        let now = Utc::now();
        let enqueued_at = now - TimeDelta::seconds(30);

        test_metrics
            .metrics
            .job_started("SyncTasks", 1, Some(enqueued_at), now);
        test_metrics
            .metrics
            .job_started("SyncTasks", 2, Some(enqueued_at), now);
        test_metrics.metrics.job_started("SyncTasks", 1, None, now);

        assert_eq!(
            test_metrics.values("job.wait.duration"),
            vec![(attrs(&[(attr::JOB_NAME, "SyncTasks")]), 30.0)]
        );
    }

    #[test]
    fn test_retries_are_counted_from_the_second_attempt() {
        let test_metrics = TestMetrics::new();
        let now = Utc::now();

        test_metrics.metrics.job_started("SyncTasks", 1, None, now);
        test_metrics.metrics.job_started("SyncTasks", 2, None, now);
        test_metrics.metrics.job_started("SyncTasks", 3, None, now);

        assert_eq!(
            test_metrics.values("job.retries"),
            vec![(attrs(&[(attr::JOB_NAME, "SyncTasks")]), 2.0)]
        );
    }

    #[test]
    fn test_execution_is_tagged_with_its_outcome() {
        let test_metrics = TestMetrics::new();

        test_metrics
            .metrics
            .job_finished("SyncTasks", Duration::from_millis(1500), None);
        test_metrics
            .metrics
            .job_finished("SyncTasks", Duration::from_millis(500), Some("timeout"));

        assert_eq!(
            test_metrics.values("job.execution.duration"),
            vec![
                (
                    attrs(&[
                        (attr::JOB_NAME, "SyncTasks"),
                        (attr::JOB_OUTCOME, "failure")
                    ]),
                    0.5
                ),
                (
                    attrs(&[
                        (attr::JOB_NAME, "SyncTasks"),
                        (attr::JOB_OUTCOME, "success")
                    ]),
                    1.5
                ),
            ]
        );
        assert_eq!(
            test_metrics.values("job.executions"),
            vec![
                (
                    attrs(&[
                        (attr::ERROR_TYPE, "timeout"),
                        (attr::JOB_NAME, "SyncTasks"),
                        (attr::JOB_OUTCOME, "failure")
                    ]),
                    1.0
                ),
                (
                    attrs(&[
                        (attr::JOB_NAME, "SyncTasks"),
                        (attr::JOB_OUTCOME, "success")
                    ]),
                    1.0
                ),
            ]
        );
    }

    #[test]
    fn test_queue_sample_is_recorded_as_gauges() {
        let test_metrics = TestMetrics::new();

        test_metrics.metrics.queue_sampled(
            "jobs",
            &QueueSample {
                pending: 3,
                running: 1,
                failed: 2,
                dead: 0,
                oldest_pending_age: 42.5,
            },
        );

        let queue_attrs = |state| {
            attrs(&[
                (attr::MESSAGING_DESTINATION_NAME, "jobs"),
                (attr::JOB_QUEUE_STATE, state),
            ])
        };
        assert_eq!(
            test_metrics.values("job.queue.length"),
            vec![
                (queue_attrs("dead"), 0.0),
                (queue_attrs("failed"), 2.0),
                (queue_attrs("pending"), 3.0),
                (queue_attrs("running"), 1.0),
            ]
        );
        assert_eq!(
            test_metrics.values("job.queue.oldest_pending.age"),
            vec![(attrs(&[(attr::MESSAGING_DESTINATION_NAME, "jobs")]), 42.5)]
        );
    }
}
