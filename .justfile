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

## Build recipes
build-release-all: build-release
    just web build-release
    just api build-release

## Dev recipes
check-all: check check-pii-logging
    just web check
    just api check

# Fails when a `Pii` value is exposed inside a tracing macro or a
# `#[tracing::instrument]` attribute: that sends personal data to telemetry.
# Text-based, so a value exposed into a variable first and logged later slips
# through (the exporter-side redaction stays the safety net).
check-pii-logging:
    #!/usr/bin/env bash
    set -euo pipefail
    macro='\b(?:trace|debug|info|warn|error|event|span|(?:trace|debug|info|warn|error)_span)!\([^;]*?\.(?:expose|into_inner)\(\)'
    attribute='#\[(?:tracing::)?instrument\([^\]]*?\.(?:expose|into_inner)\(\)'
    if rg --multiline --line-number --glob '*.rs' "$macro|$attribute" src api web; then
        echo "error: Pii::expose()/into_inner() inside tracing leaks personal data to telemetry" >&2
        exit 1
    fi

format-all:
    cargo fmt --all

check-format:
    cargo fmt --all --check

lint-dockerfile:
    hadolint Dockerfile

check-deny:
    cargo deny --workspace check advisories sources bans licenses

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
    mise daemons start
    exec mise daemons tui

# Start the stack detached + headless (no TUI) — for agents / CI / non-interactive shells.
# Brings up postgresql + redis; app services (ui-*) stay stopped, start them with `just start`.
# Idempotent: a no-op if the stack is already running. Tear down with `just down`.
run-detached:
    mise daemons start

# Stop the stack started by `run` or `run-detached`.
@down:
    mise daemons stop

@start service:
    mise daemons start {{ service }}

@stop service:
    mise daemons stop {{ service }}

@logs service:
    mise daemons logs {{ service }} -n 100 --follow

status:
    #!/usr/bin/env bash
    echo "╭──────────────────────────────────────────────────────────────╮"
    echo "│                    Service Status                            │"
    echo "╰──────────────────────────────────────────────────────────────╯"
    echo ""
    json=$(mise daemons ls --json)
    for service in postgresql redis ui-api ui-workers ui-web; do
        status=$(echo "$json" | jq -r --arg service "$service" '.[] | select(.name == $service) | .status // "unknown"')
        if [ "$status" = "running" ]; then
            icon="✅"
        elif [ "$status" = "available" ] || [ "$status" = "stopped" ]; then
            icon="⏸️ "
        else
            icon="❌"
        fi
        printf "%s %-15s %s\n" "$icon" "$service" "$status"
    done

open:
    open "http://localhost:${DX_SERVE_PORT:-8000}"
