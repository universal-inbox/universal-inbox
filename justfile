export NANGO_POSTGRES_PASSWORD := "nango"
export NANGO_POSTGRES_USER := "nango"
export NANGO_POSTGRES_DB := "nango"

default:
    @just --choose

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
    docker build -t dax42/universal-inbox:latest .

publish-container:
    docker buildx build \
      --push \
      --platform linux/arm64/v8,linux/amd64 \
      --tag dax42/universal-inbox:latest \
      --tag dax42/universal-inbox:$(git rev-parse HEAD) \
      .
    echo "🚀 Docker image dax42/universal-inbox published with tag $(git rev-parse HEAD)"

## Dev recipes
run-db:
    process-compose -f .devbox/virtenv/redis/process-compose.yaml -f .devbox/virtenv/postgresql_15/process-compose.yaml -p 9999

check:
    cargo clippy --tests -- -D warnings

check-all: check
    just web/check
    just api/check

format:
    cargo fmt --all

check-format:
    cargo fmt --all --check

## Test recipes
test test-filter="" $RUST_LOG="info":
    cargo nextest run {{test-filter}}

test-all: test
    just web/test
    just api/test

test-coverage:
    cargo llvm-cov nextest --all-features --lcov --output-path lcov.info

## Run recipes
migrate-db:
    just api/migrate-db

run-api:
    just api/run

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
        --network universal-inbox \
        --rm \
        -p 3003:3003 \
        --name nango-server \
        nangohq/nango-server:0.32.10

run-all:
    process-compose -f .devbox/virtenv/redis/process-compose.yaml -f .devbox/virtenv/postgresql_15/process-compose.yaml -f process-compose.yaml -p 9999
