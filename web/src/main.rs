extern crate console_error_panic_hook;

use cfg_if::cfg_if;
use std::panic;
use universal_inbox_web::app;

cfg_if! {
    if #[cfg(feature = "console_log")] {
        fn init_log() {
            use log::Level;
            console_log::init_with_level(Level::Trace).expect("error initializing log");
        }
    } else {
        fn init_log() {}
    }
}

fn main() {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    init_log();
    dioxus::web::launch(app);
}
