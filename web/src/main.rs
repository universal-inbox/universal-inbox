extern crate console_error_panic_hook;

use cfg_if::cfg_if;
use log::{info, Level};
use std::panic;
use universal_inbox_web::app;

cfg_if! {
    if #[cfg(debug_assertions)] {
        const LOG_LEVEL: Level = Level::Trace;
    } else {
        const LOG_LEVEL: Level = Level::Info;
    }
}

cfg_if! {
    if #[cfg(feature = "console_log")] {
        fn init_log() {
            console_log::init_with_level(LOG_LEVEL).expect("error initializing log");
            info!("Log level set to {}", LOG_LEVEL.to_string());
        }
    } else {
        fn init_log() {}
    }
}

fn main() {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    init_log();
    dioxus_web::launch(app);
}
