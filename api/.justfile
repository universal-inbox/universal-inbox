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

test test-filter="" $RUST_LOG="info":
    cargo nextest run -E 'not binary(browser)' --color always {{test-filter}}

test-browser test-filter="" $RUST_LOG="info":
    #!/usr/bin/env bash

    set -euo pipefail
    
    cd ..
    just web build-ci
    cd -
    cargo nextest run -E 'binary(browser)' --color always {{test-filter}}

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

anonymize-db:
    cargo run -- test anonymize-db
    
test-ci:
    cargo nextest run --profile ci -E 'not binary(browser)'

test-ci-browser:
    cargo nextest run --profile ci-browser -E 'binary(browser)'

install-tools:
    #!/usr/bin/env bash
    set -euo pipefail

    # Install Playwright browsers for browser tests (version must match playwright-rs crate)
    PLAYWRIGHT_VERSION="1.56.1"
    PLAYWRIGHT_CACHE_DIR="${PLAYWRIGHT_BROWSERS_PATH:-${HOME}/Library/Caches/ms-playwright}"
    if [ "$(uname)" = "Linux" ]; then
        PLAYWRIGHT_CACHE_DIR="${PLAYWRIGHT_BROWSERS_PATH:-${HOME}/.cache/ms-playwright}"
    fi

    if [ -d "${PLAYWRIGHT_CACHE_DIR}/chromium-1194" ]; then
        echo "Playwright Chromium already installed, skipping."
    else
        echo "Installing Playwright Chromium ${PLAYWRIGHT_VERSION}..."
        npx --yes playwright@${PLAYWRIGHT_VERSION} install chromium
    fi
