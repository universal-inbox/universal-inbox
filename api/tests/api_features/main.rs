#![recursion_limit = "256"]
#[path = "../common/mod.rs"]
mod common;
#[path = "../api/helpers/mod.rs"]
#[allow(dead_code, unused_imports)]
mod helpers;
mod state;
mod steps;
mod world;

fn main() {
    common::scenarios::main::<world::ApiWorld>(|path| !path.starts_with("browser"));
}
