#![recursion_limit = "256"]
#[path = "../common/mod.rs"]
mod common;
#[path = "../browser/helpers.rs"]
#[allow(dead_code)]
mod helpers;
mod steps;
mod world;

fn main() {
    common::scenarios::main::<world::BrowserWorld>(|path| path.starts_with("browser"));
}
