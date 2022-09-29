use log;
use tracing::{subscriber::set_global_default, Subscriber};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::fmt::TestWriter;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter};

pub fn get_subscriber(env_filter_str: &str) -> impl Subscriber + Send + Sync {
    let formatting_layer =
        BunyanFormattingLayer::new("universal-inbox-api".into(), TestWriter::new);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter_str));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
}

pub fn init_subscriber(
    subscriber: impl Subscriber + Send + Sync,
    log_level_filter: log::LevelFilter,
) {
    LogTracer::init_with_filter(log_level_filter).expect("Failed to set logger");
    set_global_default(subscriber).expect("Failed to set subscriber");
}
