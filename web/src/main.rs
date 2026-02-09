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

cfg_if! {
    if #[cfg(feature = "console_log")] {
        fn init_log() {
            console_log::init_with_level(LOG_LEVEL).expect("error initializing log");
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
