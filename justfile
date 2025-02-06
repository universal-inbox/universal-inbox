import? 'justfile.local'

export NANGO_POSTGRES_PASSWORD := "nango"
export NANGO_POSTGRES_USER := "nango"
export NANGO_POSTGRES_DB := "nango"

default:
    @just --choose

## Setup recipes
init-db:
  #!/usr/bin/env bash

  [ -d .devbox/virtenv/postgresql_15/data ] || initdb --username=postgres --pwfile=<(echo password)

install-rust-toolchain:
  #!/usr/bin/env bash

  rustup show active-toolchain | grep -q "^$RUST_TOOLCHAIN_VERSION-" || rustup default $RUST_TOOLCHAIN_VERSION
  for toolchain in $(rustup toolchain list | grep -v $(rustup show active-toolchain | awk '{ print $1 }')); do
    rustup toolchain uninstall $toolchain
  done
  rustup target list --installed | grep -q '^wasm32-unknown-unknown$' || rustup target add wasm32-unknown-unknown
  rustup component list --installed | grep -q '^rust-analyzer-' || rustup component add rust-analyzer
  rustup component list --installed | grep -q '^llvm-tools-' || rustup component add llvm-tools-preview

install-rust-tools:
  #!/usr/bin/env bash
  if [ -z "$DOCKER_BUILD" ]; then
    cargo binstall -y cargo-llvm-cov --version 0.6.16
    cargo binstall -y dioxus-cli --version 0.6.2
  fi

## Build recipes
clean:
    cargo clean

build:
    cargo build

build-all: build
    just web/build
    just api/build

build-release:
    cargo build --release

build-release-all: build-release
    just web/build-release
    just api/build-release

build-container:
    just docker/build

publish-container:
    just docker/publish

## Dev recipes
run-db:
    process-compose -f .devbox/virtenv/redis/process-compose.yaml -f .devbox/virtenv/postgresql_15/process-compose.yaml -p 9999

check:
    cargo clippy --tests -- -D warnings

check-all: check
    just web/check
    just api/check

check-unused-dependencies:
    cargo machete --with-metadata Cargo.toml web/Cargo.toml api/Cargo.toml

format:
    cargo fmt --all

format-sql sql-files="api/migrations":
    sqlfluff fix --disable-progress-bar --dialect postgres {{sql-files}}

check-format:
    cargo fmt --all --check

check-commit: format check-unused-dependencies check-all test-all

## Test recipes
test test-filter="" $RUST_LOG="info":
    cargo nextest run {{test-filter}}

test-all: test
    just web/test
    just api/test

test-ci:
    cargo nextest run --profile ci

test-coverage:
    cargo llvm-cov nextest --all-features --lcov --output-path lcov.info --profile ci

## Run recipes
migrate-db:
    just api/migrate-db

run-api:
    just api/run-api

run-workers:
    just api/run-workers

clear-cache:
    just api/clear-cache

run-web:
    just web/run

sync-notifications:
    just api/sync-notifications

sync-tasks:
    just api/sync-tasks


@create-docker-network:
    #!/usr/bin/env bash
    docker network ls | awk '{print $2}' | grep -q '^universal-inbox$' \
        || docker network create universal-inbox;

run-nango-db: create-docker-network
    #!/usr/bin/env bash
    docker volume ls | awk '{print $2}' | grep -q '^nango-db$' \
        || docker volume create nango-db;

    docker run \
        -e POSTGRES_PASSWORD=$NANGO_POSTGRES_PASSWORD \
        -e POSTGRES_USER=$NANGO_POSTGRES_USER \
        -e POSTGRES_DB=$NANGO_POSTGRES_DB \
        --network universal-inbox \
        --rm \
        --mount type=volume,source=nango-db,target=/var/lib/postgresql/data \
        -p 25432:5432 \
        --name nango-db \
        postgres:15

run-nango-server: create-docker-network
    #!/usr/bin/env bash
    docker run \
        -e TELEMETRY=false \
        -e SERVER_PORT=3003 \
        -e NANGO_SERVER_URL="https://oauth-dev.universal-inbox.com" \
        -e LOG_LEVEL="info" \
        -e NANGO_SECRET_KEY="c9c9d3bb-4a08-4dcd-a674-09b3781b7d05" \
        -e NANGO_ENCRYPTION_KEY="ITbLZfSOuTaqglhgh9tZROu0GUBMRoEwAmGK9K7x56Q=" \
        -e NANGO_DASHBOARD_USERNAME=nango \
        -e NANGO_DASHBOARD_PASSWORD=nango \
        -e NANGO_DB_HOST="nango-db" \
        -e NANGO_DB_PORT=5432 \
        -e NANGO_DB_NAME=$NANGO_POSTGRES_DB \
        -e NANGO_DB_USER=$NANGO_POSTGRES_USER \
        -e NANGO_DB_PASSWORD=$NANGO_POSTGRES_PASSWORD \
        -e NANGO_DB_SSL=false \
        --network universal-inbox \
        --rm \
        --platform linux/amd64 \
        -p 3003:3003 \
        --name nango-server \
        nangohq/nango-server:hosted-02c1ee2bcbd92741057582169c95c4157ba98262

run-all:
    process-compose \
        -f .devbox/virtenv/redis/process-compose.yaml \
        -f .devbox/virtenv/postgresql_15/process-compose.yaml \
        -f process-compose.yaml \
        -p 9999
