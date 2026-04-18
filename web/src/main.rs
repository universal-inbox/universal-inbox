extern crate console_error_panic_hook;

use cfg_if::cfg_if;
use log::{Level, info};
use std::panic;
use universal_inbox_web::App;

cfg_if! {
    if #[cfg(debug_assertions)] {
        const LOG_LEVEL: Level = Level::Trace;
    } else {
        const LOG_LEVEL: Level = Level::Debug;
    }
}

#[cfg(feature = "console_log")]
mod filtered_logger {
    use log::{Level, LevelFilter, Log, Metadata, Record};

    const MODULE_CEILINGS: &[(&str, LevelFilter)] = &[("html5ever", LevelFilter::Warn)];

    pub struct FilteredLogger {
        pub default_level: Level,
    }

    impl Log for FilteredLogger {
        fn enabled(&self, metadata: &Metadata) -> bool {
            let ceiling = MODULE_CEILINGS
                .iter()
                .find(|(prefix, _)| metadata.target().starts_with(prefix))
                .map(|(_, level)| *level)
                .unwrap_or_else(|| self.default_level.to_level_filter());
            metadata.level().to_level_filter() <= ceiling
        }

        fn log(&self, record: &Record) {
            if self.enabled(record.metadata()) {
                console_log::log(record);
            }
        }

        fn flush(&self) {}
    }
}

cfg_if! {
    if #[cfg(feature = "console_log")] {
        static LOGGER: filtered_logger::FilteredLogger = filtered_logger::FilteredLogger { default_level: LOG_LEVEL };

        fn init_log() {
            log::set_logger(&LOGGER).expect("error initializing log");
            log::set_max_level(LOG_LEVEL.to_level_filter());
            info!("Log level set to {}", LOG_LEVEL);
        }
    } else {
        fn init_log() {}
    }
}

fn main() {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    init_log();
    dioxus::launch(App);
}
