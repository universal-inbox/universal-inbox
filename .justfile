import ".common-rust.justfile"

import? '.justfile.local'

mod web
mod api
mod docker
mod doc

[private]
default:
    @just --choose

## Environment info
print-env-info:
    #!/usr/bin/env bash
    echo "╭──────────────────────────────────────────────────────────────╮"
    echo "│              Universal Inbox Environment Info                │"
    echo "╰──────────────────────────────────────────────────────────────╯"
    echo ""
    echo "🌐 Web UI:           http://localhost:${DX_SERVE_PORT:-8080}"
    echo "🔌 API:              http://localhost:${API_PORT:-8000}"
    echo "🐘 PostgreSQL:       postgres://postgres:password@127.0.0.1:${PGPORT:-5432}/universal-inbox"
    echo "📮 Redis:            redis://127.0.0.1:${REDIS_PORT:-6379}"

## Setup recipes
init-db:
  #!/usr/bin/env bash

  [ -f .devbox/virtenv/postgresql_17/data/postgresql.conf ] \
    || initdb -D .devbox/virtenv/postgresql_17/data --username=postgres --pwfile=<(echo password)

install-rust-toolchain:
  #!/usr/bin/env bash

  rustup show active-toolchain | grep -q "^$RUST_TOOLCHAIN_VERSION-" || rustup default $RUST_TOOLCHAIN_VERSION
  for toolchain in $(rustup toolchain list | grep -v $(rustup show active-toolchain | awk '{ print $1 }')); do
    rustup toolchain uninstall $toolchain
  done
  rustup target list --installed | grep -q '^wasm32-unknown-unknown$' || rustup target add wasm32-unknown-unknown
  rustup component list --installed | grep -q '^rust-analyzer-' || rustup component add rust-analyzer
  rustup component list --installed | grep -q '^llvm-tools-' || rustup component add llvm-tools-preview

install-tools:
  #!/usr/bin/env bash

  if [ -z "$DOCKER_BUILD" ]; then
    cargo binstall -y cargo-llvm-cov --version 0.6.16
  fi
  just api install-tools

## Build recipes
build-release-all: build-release
    just web build-release
    just api build-release

## Dev recipes
check-all: check
    just web check
    just api check

format-all:
    cargo fmt --all

check-format:
    cargo fmt --all --check

lint-dockerfile:
    hadolint Dockerfile

@check-commit:
    env SKIP= prek run -a

## Test recipes
test-all: test
    just web test
    just api test

## Run recipes
@create-docker-network:
    #!/usr/bin/env bash
    docker network ls | awk '{print $2}' | grep -q '^universal-inbox$' \
        || docker network create universal-inbox;

run:
    #!/usr/bin/env bash

    process-compose \
        -f .devbox/virtenv/redis/process-compose.yaml \
        -f process-compose-pg.yaml \
        -f process-compose.yaml \
        -p ${PROCESS_COMPOSE_PORT:-9999}

@start service:
    process-compose -p ${PROCESS_COMPOSE_PORT:-9999} process start {{ service }}

@stop service:
    process-compose -p ${PROCESS_COMPOSE_PORT:-9999} process stop {{ service }}

@logs service:
    process-compose -p ${PROCESS_COMPOSE_PORT:-9999} process logs -n 100 -f {{ service }}

status:
    #!/usr/bin/env bash
    PC_PORT=${PROCESS_COMPOSE_PORT:-9999}
    echo "╭──────────────────────────────────────────────────────────────╮"
    echo "│                    Service Status                            │"
    echo "╰──────────────────────────────────────────────────────────────╯"
    echo ""
    for service in postgresql redis ui-api ui-workers ui-web; do
        json=$(process-compose -p $PC_PORT process get -o json "$service" 2>/dev/null)
        status=$(echo "$json" | jq -r '.[0].status // "Unknown"')
        is_running=$(echo "$json" | jq -r '.[0].is_running // false')
        if [ "$is_running" = "true" ]; then
            icon="✅"
        elif [ "$status" = "Disabled" ]; then
            icon="⏸️ "
        else
            icon="❌"
        fi
        printf "%s %-15s %s\n" "$icon" "$service" "$status"
    done
