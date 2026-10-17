//! Metrics of the realtime events, per API pod. The "stream closed" log gives the per-stream
//! view.

use std::sync::OnceLock;

use opentelemetry::{
    KeyValue,
    metrics::{Counter, Meter, UpDownCounter},
};

use universal_inbox::realtime::RealtimeEventKind;

use crate::observability::attr;

/// Why a stream was told to refetch everything.
#[derive(Debug, Clone, Copy)]
pub enum ResyncReason {
    /// The stream did not read its events fast enough
    Lagged,
    /// The pod subscribed again to the events, some may have been lost meanwhile
    Resubscribed,
}

impl ResyncReason {
    fn as_str(self) -> &'static str {
        match self {
            ResyncReason::Lagged => "lagged",
            ResyncReason::Resubscribed => "resubscribed",
        }
    }
}

pub struct RealtimeMetrics {
    published_events: Counter<u64>,
    publish_failures: Counter<u64>,
    delivered_events: Counter<u64>,
    dropped_events: Counter<u64>,
    resyncs: Counter<u64>,
    open_streams: UpDownCounter<i64>,
}

impl RealtimeMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            published_events: meter
                .u64_counter("realtime.events.published")
                .with_description("Realtime events published")
                .build(),
            publish_failures: meter
                .u64_counter("realtime.events.publish_failures")
                .with_description("Realtime events that could not be published")
                .build(),
            delivered_events: meter
                .u64_counter("realtime.events.delivered")
                .with_description("Realtime events sent to an open event stream")
                .build(),
            dropped_events: meter
                .u64_counter("realtime.events.dropped")
                .with_description(
                    "Hub messages (events or resync orders) lost by event streams too slow to read them",
                )
                .build(),
            resyncs: meter
                .u64_counter("realtime.streams.resyncs")
                .with_description("Event streams told to refetch everything")
                .build(),
            open_streams: meter
                .i64_up_down_counter("realtime.streams.open")
                .with_description("Open event streams")
                .build(),
        }
    }

    pub fn event_published(&self, kind: RealtimeEventKind) {
        self.published_events.add(
            1,
            &[KeyValue::new(attr::REALTIME_EVENT_KIND, kind.to_string())],
        );
    }

    pub fn publish_failed(&self, events_count: u64, error_type: &'static str) {
        self.publish_failures
            .add(events_count, &[KeyValue::new(attr::ERROR_TYPE, error_type)]);
    }

    pub fn events_delivered(&self, streams_count: u64) {
        if streams_count > 0 {
            self.delivered_events.add(streams_count, &[]);
        }
    }

    pub fn events_dropped(&self, events_count: u64) {
        self.dropped_events.add(events_count, &[]);
    }

    pub fn resynced(&self, streams_count: u64, reason: ResyncReason) {
        if streams_count > 0 {
            self.resyncs.add(
                streams_count,
                &[KeyValue::new(attr::REALTIME_RESYNC_REASON, reason.as_str())],
            );
        }
    }

    pub fn stream_opened(&self) {
        self.open_streams.add(1, &[]);
    }

    pub fn stream_closed(&self) {
        self.open_streams.add(-1, &[]);
    }
}

/// The realtime metrics of the process, recorded with the global meter provider set up by
/// `observability` (no-ops without it). The instruments are created on the first call: it
/// must come after the subscriber setup, which sets the meter provider.
pub fn realtime_metrics() -> &'static RealtimeMetrics {
    static REALTIME_METRICS: OnceLock<RealtimeMetrics> = OnceLock::new();
    REALTIME_METRICS
        .get_or_init(|| RealtimeMetrics::new(&opentelemetry::global::meter("universal-inbox")))
}

/// Metrics recorded in memory, for the tests of the code recording them.
#[cfg(test)]
pub(crate) mod test_support {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{
        InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
        data::{AggregatedMetrics, MetricData},
    };

    use super::*;

    pub(crate) struct TestMetrics {
        exporter: InMemoryMetricExporter,
        provider: SdkMeterProvider,
        pub(crate) metrics: &'static RealtimeMetrics,
    }

    impl TestMetrics {
        pub(crate) fn new() -> Self {
            let exporter = InMemoryMetricExporter::default();
            let provider = SdkMeterProvider::builder()
                .with_reader(PeriodicReader::builder(exporter.clone()).build())
                .build();
            // Leaked: a test lives as long as its process
            let metrics = Box::leak(Box::new(RealtimeMetrics::new(&provider.meter("test"))));
            Self {
                exporter,
                provider,
                metrics,
            }
        }

        /// The data points (attributes, value) of the sum `name` recorded so far.
        pub(crate) fn sum(&self, name: &str) -> Vec<(Vec<(String, String)>, i64)> {
            self.provider.force_flush().unwrap();
            let mut points = vec![];
            // The exporter keeps every export: the last one holds the current (cumulative) sums
            let finished_metrics = self.exporter.get_finished_metrics().unwrap();
            if let Some(resource_metrics) = finished_metrics.last() {
                for scope_metrics in resource_metrics.scope_metrics() {
                    for metric in scope_metrics
                        .metrics()
                        .filter(|metric| metric.name() == name)
                    {
                        match metric.data() {
                            AggregatedMetrics::U64(MetricData::Sum(sum)) => {
                                points.extend(sum.data_points().map(|point| {
                                    (attributes(point.attributes()), point.value() as i64)
                                }))
                            }
                            AggregatedMetrics::I64(MetricData::Sum(sum)) => points.extend(
                                sum.data_points()
                                    .map(|point| (attributes(point.attributes()), point.value())),
                            ),
                            data => panic!("Unexpected data for {name}: {data:?}"),
                        }
                    }
                }
            }
            points.sort();
            points
        }
    }

    impl Drop for TestMetrics {
        fn drop(&mut self) {
            let _ = self.provider.shutdown();
        }
    }

    fn attributes<'a>(attributes: impl Iterator<Item = &'a KeyValue>) -> Vec<(String, String)> {
        attributes
            .map(|kv| (kv.key.to_string(), kv.value.to_string()))
            .collect()
    }

    pub(crate) fn attribute(key: &str, value: &str) -> Vec<(String, String)> {
        vec![(key.to_string(), value.to_string())]
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{
        test_support::{TestMetrics, attribute},
        *,
    };

    #[test]
    fn test_realtime_metrics_are_recorded() {
        let test_metrics = TestMetrics::new();
        let metrics = test_metrics.metrics;

        metrics.event_published(RealtimeEventKind::NotificationsChanged);
        metrics.event_published(RealtimeEventKind::NotificationsChanged);
        metrics.event_published(RealtimeEventKind::TasksChanged);
        metrics.publish_failed(3, "timeout");
        metrics.events_delivered(2);
        metrics.events_delivered(0);
        metrics.events_dropped(5);
        metrics.resynced(4, ResyncReason::Resubscribed);
        metrics.resynced(1, ResyncReason::Lagged);
        metrics.stream_opened();
        metrics.stream_opened();
        metrics.stream_closed();

        assert_eq!(
            test_metrics.sum("realtime.events.published"),
            vec![
                (
                    attribute(attr::REALTIME_EVENT_KIND, "notifications.changed"),
                    2
                ),
                (attribute(attr::REALTIME_EVENT_KIND, "tasks.changed"), 1),
            ]
        );
        assert_eq!(
            test_metrics.sum("realtime.events.publish_failures"),
            vec![(attribute(attr::ERROR_TYPE, "timeout"), 3)]
        );
        assert_eq!(
            test_metrics.sum("realtime.events.delivered"),
            vec![(vec![], 2)]
        );
        assert_eq!(
            test_metrics.sum("realtime.events.dropped"),
            vec![(vec![], 5)]
        );
        assert_eq!(
            test_metrics.sum("realtime.streams.resyncs"),
            vec![
                (attribute(attr::REALTIME_RESYNC_REASON, "lagged"), 1),
                (attribute(attr::REALTIME_RESYNC_REASON, "resubscribed"), 4),
            ]
        );
        assert_eq!(test_metrics.sum("realtime.streams.open"), vec![(vec![], 1)]);
    }
}
