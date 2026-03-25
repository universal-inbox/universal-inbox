set fallback

[private]
default:
    @just --choose

run:
    mdbook serve --open

build: install
    mdbook build

install:
    cargo binstall -y mdbook-image-size --version 0.2.1
    cargo binstall -y mdbook-classy --version 0.1.0

test:
    mdbook test

# Regenerate every documentation screenshot by driving a headless Chromium
# against the locally running Universal Inbox. Requires the dev stack to be
# up (`just start ui-api`, `just start ui-workers`, `just start ui-web`) and
# Playwright Chromium installed (`just api::install-tools`).
#
# Devbox assigns random ports to every service so the recipe relies on the env
# vars exposed by `.envrc` (API_PORT, DX_SERVE_PORT, UNIVERSAL_INBOX__*). If
# direnv is installed but the recipe is invoked from a shell where the vars
# aren't loaded, we auto-source them from the worktree root.
#
# Usage:
#   just doc::update-screenshots                   # regenerate everything
#   just doc::update-screenshots inbox-screen      # filter by manifest name
#   just doc::update-screenshots inbox-screen,login-page
update-screenshots only="":
    #!/usr/bin/env bash
    set -euo pipefail

    # Auto-load env vars from the worktree's .envrc if they're missing. The dev
    # stack uses random ports (devbox) — without these we'd hit the wrong host.
    if [ -z "${API_PORT:-}" ] || [ -z "${DX_SERVE_PORT:-}" ]; then
        if command -v direnv >/dev/null 2>&1; then
            eval "$(direnv export bash 2>/dev/null || true)"
        fi
    fi

    if [ -z "${API_PORT:-}" ] || [ -z "${DX_SERVE_PORT:-}" ]; then
        echo "ERROR: API_PORT and/or DX_SERVE_PORT are not set."
        echo "Run from a devbox shell, or 'direnv allow' the worktree, before invoking this recipe."
        exit 1
    fi

    curl -fsS "http://localhost:${API_PORT}/ping" >/dev/null \
        || { echo "API not running on :${API_PORT}. Run: just start ui-api"; exit 1; }
    curl -fsS "http://localhost:${DX_SERVE_PORT}/" >/dev/null \
        || { echo "Web not running on :${DX_SERVE_PORT}. Run: just start ui-web"; exit 1; }

    only_flag=""
    if [ -n "{{only}}" ]; then
        only_flag="--only={{only}}"
    fi
    cd ../api
    just generate-doc-screenshots "http://localhost:${DX_SERVE_PORT}" "../doc/src" "$only_flag"
