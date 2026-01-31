import ".common-rust.justfile"

import? '.justfile.local'

export NANGO_POSTGRES_PASSWORD := "nango"
export NANGO_POSTGRES_USER := "nango"
export NANGO_POSTGRES_DB := "nango"

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
    echo "🔗 Nango:            http://localhost:${NANGO_PORT:-3003}"
    echo ""
    echo "📁 Branch-specific containers:"
    echo "   Nango DB:         ${NANGO_DB_CONTAINER_NAME:-nango-db} (port ${NANGO_DB_PORT:-25432})"
    echo "   Nango Server:     ${NANGO_CONTAINER_NAME:-nango-server}"

## Setup recipes
init-db:
  #!/usr/bin/env bash

  [ -f .devbox/virtenv/postgresql_17/data/postgresql.conf ] || initdb --username=postgres --pwfile=<(echo password)

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

run-nango-db: create-docker-network
    #!/usr/bin/env bash
    docker volume ls | awk '{print $2}' | grep -q '^nango-db$' \
        || docker volume create ${NANGO_DB_VOLUME_NAME:-nango-db};

    docker run \
        -e POSTGRES_PASSWORD=$NANGO_POSTGRES_PASSWORD \
        -e POSTGRES_USER=$NANGO_POSTGRES_USER \
        -e POSTGRES_DB=$NANGO_POSTGRES_DB \
        --network universal-inbox \
        --rm \
        --mount type=volume,source=${NANGO_DB_VOLUME_NAME:-nango-db},target=/var/lib/postgresql/data \
        -p ${NANGO_DB_PORT:-25432}:5432 \
        --name ${NANGO_DB_CONTAINER_NAME:-nango-db} \
        postgres:15

run-nango-server: create-docker-network
    #!/usr/bin/env bash

    arch=$(uname -m)
    if [ "$arch" = "aarch64" ]; then
        image="dax42/nango-server:0.32.10"
    else
        image="nangohq/nango-server:0.32.10"
    fi
    echo "Using image: $image"
    
    docker run \
        -e TELEMETRY=false \
        -e SERVER_PORT=3003 \
        -e NANGO_SERVER_URL="http://localhost:${NANGO_PORT:-3003}" \
        -e LOG_LEVEL="info" \
        -e NANGO_SECRET_KEY="c9c9d3bb-4a08-4dcd-a674-09b3781b7d05" \
        -e NANGO_ENCRYPTION_KEY="ITbLZfSOuTaqglhgh9tZROu0GUBMRoEwAmGK9K7x56Q=" \
        -e NANGO_DASHBOARD_USERNAME=nango \
        -e NANGO_DASHBOARD_PASSWORD=nango \
        -e NANGO_DB_HOST="${NANGO_DB_CONTAINER_NAME:-nango-db}" \
        -e NANGO_DB_PORT=5432 \
        -e NANGO_DB_NAME=$NANGO_POSTGRES_DB \
        -e NANGO_DB_USER=$NANGO_POSTGRES_USER \
        -e NANGO_DB_PASSWORD=$NANGO_POSTGRES_PASSWORD \
        -e NANGO_DB_SSL=false \
        --network universal-inbox \
        --rm \
        -p ${NANGO_PORT:-3003}:3003 \
        --name ${NANGO_CONTAINER_NAME:-nango-server} \
        "$image"

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
    for service in postgresql redis nango-db nango-server ui-api ui-workers ui-web; do
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
