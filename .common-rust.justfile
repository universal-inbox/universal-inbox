# -*- just-ts -*-

## Build recipes
clean:
    cargo clean

build:
    cargo build

build-release:
    cargo build --release

## Dev recipes
check:
    cargo clippy --tests -- -D warnings

## Test recipes
test test-filter="" $RUST_LOG="info":
    cargo nextest run --color always {{test-filter}}

test-ci:
    cargo nextest run --profile ci

test-coverage:
    cargo llvm-cov nextest --all-features --lcov --output-path lcov.info --profile ci

check-unused-dependencies:
    cargo machete --with-metadata Cargo.toml

format:
    cargo fmt
