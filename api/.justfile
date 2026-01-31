set fallback
set allow-duplicate-recipes

import "../.common-rust.justfile"

[private]
default:
    @just --choose

## Dev recipes
check-db:
    cargo sqlx prepare -- --bin universal-inbox-api
    cargo check --tests

ensure-db:
    #!/usr/bin/env bash
    set -euo pipefail
    db_name=$(echo "$DATABASE_URL" | sed -E 's|.*/([^?]+).*|\1|')
    db_url_without_db=$(echo "$DATABASE_URL" | sed -E 's|/[^/]+$||')/postgres
    if ! psql "$db_url_without_db" -lqt | cut -d \| -f 1 | grep -qw "$db_name"; then
        echo "Creating database '$db_name'..."
        sqlx database setup || true
    fi

migrate-db:
    sqlx database setup

## Run recipes
run *command="serve --embed-async-workers": ensure-db
    cargo run --color always -- {{ command }}

run-api: ensure-db
    watchexec --stop-timeout 10 --debounce 500 --exts toml,rs --restart --watch src cargo run --color always -- serve

run-workers: ensure-db
    watchexec --stop-timeout 10 --debounce 500 --exts toml,rs --restart --watch src cargo run --color always -- start-workers

sync-tasks $RUST_LOG="info":
    cargo run -- sync-tasks

sync-notifications $RUST_LOG="info":
    cargo run -- sync-notifications

clear-cache:
    cargo run -- cache clear

generate-jwt-key-pair:
    cargo run -- generate-jwt-key-pair

generate-jwt-token user-email:
    cargo run -- generate-jwt-token {{user-email}}

generate-user:
    cargo run -- test generate-user
