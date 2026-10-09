#!/usr/bin/env bash
# Runs a command inside the devbox environment on a Buildkite hosted Linux agent
# (Ubuntu, root, Docker available, no Nix). Replaces the GitHub Actions
# `jetify-com/devbox-install-action` + service containers setup.
#
# Usage: .buildkite/scripts/devbox-run.sh <command> [args...]
#
# Environment:
#   CI_SERVICES=1      start Postgres and Redis containers first (API/browser tests)
#   CI_JUNIT=<path>    on exit, copy this nextest JUnit report to
#                      junit-<step key>.xml so each step uploads a distinct artifact
set -euo pipefail

if [ -n "${CI_JUNIT:-}" ]; then
  trap 'cp "$CI_JUNIT" "junit-${BUILDKITE_STEP_KEY:-report}.xml" 2>/dev/null || true' EXIT
fi

echo "--- :nix: Setting up Nix and devbox"
if [ ! -x /nix/var/nix/profiles/default/bin/nix ]; then
  curl -fsSL https://install.determinate.systems/nix | sh -s -- install linux --init none --no-confirm
fi
export PATH="/nix/var/nix/profiles/default/bin:$PATH"

if ! command -v devbox >/dev/null; then
  curl -fsSL https://get.jetify.com/devbox | bash -s -- -f
fi
devbox version

if [ "${CI_SERVICES:-}" = "1" ]; then
  echo "--- :docker: Starting Postgres and Redis"
  # Same images as the former GitHub Actions service containers. The data is
  # disposable, so trade crash durability for speed (as `just init-db` does locally).
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

echo "--- :devbox: Installing devbox packages"
devbox install

echo "+++ :rust: $*"
devbox run -- "$@"
