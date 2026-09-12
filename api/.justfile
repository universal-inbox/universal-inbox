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

# Drops the test databases. Not part of any workflow: the suite reuses a bounded pool of slot
# databases (see api/tests/common/test_db.rs), so they no longer accumulate. Use this only to
# reclaim the ~150 MB they hold, or to force a rebuild from scratch.
clean-test-db:
    #!/usr/bin/env bash
    set -Eeuo pipefail

    server_url=$(echo "$DATABASE_URL" | sed -E 's|/[^/]+$||')/postgres
    # A template must lose its IS_TEMPLATE flag before Postgres will drop it.
    psql -tAc "SELECT format('ALTER DATABASE %I IS_TEMPLATE false;', datname) \
               FROM pg_database WHERE datname LIKE 'ui\_test\_tmpl\_%'" "$server_url" \
      | psql -q "$server_url"
    psql -tAc "SELECT format('DROP DATABASE IF EXISTS %I WITH (FORCE);', datname) \
               FROM pg_database WHERE datname LIKE 'ui\_test\_%'" "$server_url" \
      | psql -q "$server_url"
    echo "🧹 Dropped the test databases"

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
    exec watchexec --stop-timeout 10 --debounce 500 --exts toml,rs --restart --watch src cargo run --color always -- serve

run-workers: ensure-db
    exec watchexec --stop-timeout 10 --debounce 500 --exts toml,rs --restart --watch src cargo run --color always -- start-workers

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

reset-password user-email:
    cargo run -- user reset-password {{user-email}}

generate-user:
    cargo run -- test generate-user

generate-empty-user:
    cargo run -- test generate-empty-user

list-users:
    cargo run -- user list

reset-user-password user-email:
    cargo run -- user reset-password {{user-email}}

connect-integration user-id provider:
    cargo run -- test connect-integration --user-id {{user-id}} --provider {{provider}}

generate-doc-screenshots base-url="http://localhost:8080" output-dir="../doc/src" *flags="":
    cargo run --features screenshots -- test generate-doc-screenshots \
        --base-url={{base-url}} --output-dir={{output-dir}} {{flags}}

record-landing-screencast output="./screen.webm" base-url="http://localhost:8080" *flags="":
    cargo run --features screenshots -- test record-landing-screencast \
        --base-url={{base-url}} --output={{output}} {{flags}}

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
    PLAYWRIGHT_VERSION="1.63.0"
    PLAYWRIGHT_CACHE_DIR="${PLAYWRIGHT_BROWSERS_PATH:-${HOME}/Library/Caches/ms-playwright}"
    if [ "$(uname)" = "Linux" ]; then
        PLAYWRIGHT_CACHE_DIR="${PLAYWRIGHT_BROWSERS_PATH:-${HOME}/.cache/ms-playwright}"
    fi

    if [ -d "${PLAYWRIGHT_CACHE_DIR}/chromium-1243" ]; then
        echo "Playwright Chromium already installed, skipping."
    else
        echo "Installing Playwright Chromium ${PLAYWRIGHT_VERSION}..."
        npx --yes playwright@${PLAYWRIGHT_VERSION} install chromium
    fi
