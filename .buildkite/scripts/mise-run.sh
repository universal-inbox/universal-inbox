#!/usr/bin/env bash
# Runs a command inside the mise environment on a Buildkite hosted Linux agent
# (Ubuntu, root, Docker available).
#
# Usage: .buildkite/scripts/mise-run.sh <command> [args...]
#
# Environment:
#   CI_SERVICES=1      start Postgres and Redis containers first (API/browser tests)
#   CI_BROWSER_DEPS=1  install the system libraries Playwright's Chromium needs
#   CI_JUNIT=<path>    on exit, copy this nextest JUnit report to
#                      junit-<step key>.xml so each step uploads a distinct artifact
set -euo pipefail

if [ -n "${CI_JUNIT:-}" ]; then
  trap 'cp "$CI_JUNIT" "junit-${BUILDKITE_STEP_KEY:-report}.xml" 2>/dev/null || true' EXIT
fi

export MISE_YES=1
export MISE_LOCKED=1
export PATH="$HOME/.local/bin:$PATH"

echo "--- :package: Installing build prerequisites"
apt-get update -qq
apt-get install -y -qq --no-install-recommends build-essential pkg-config libssl-dev curl ca-certificates git

echo "--- :toolbox: Setting up mise"
if ! command -v mise >/dev/null; then
  bash dev/install-mise.sh
fi
mise --version

if [ "${CI_SERVICES:-}" = "1" ]; then
  echo "--- :docker: Starting Postgres and Redis"
  docker run -d --name postgres -p 5432:5432 -e POSTGRES_PASSWORD=password postgres:15.1 \
    -c fsync=off -c synchronous_commit=off -c full_page_writes=off \
    -c max_wal_size=4GB -c checkpoint_timeout=3600s
  docker run -d --name redis -p 6379:6379 redis
  for _ in $(seq 1 60); do
    if docker exec postgres pg_isready -U postgres >/dev/null 2>&1 \
      && docker exec redis redis-cli ping >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  docker exec postgres pg_isready -U postgres
  docker exec redis redis-cli ping
fi

echo "--- :toolbox: Installing mise tools"
mise install

if [ "${CI_BROWSER_DEPS:-}" = "1" ]; then
  echo "--- :chromium: Installing Chromium system libraries"
  mise exec -- playwright install --with-deps chromium
fi

echo "+++ :rust: $*"
mise exec -- "$@"
