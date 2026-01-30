set fallback
set allow-duplicate-recipes

import "../.common-rust.justfile"

[private]    
default:
    @just --choose

## Build recipes
build:
    trunk build --features trunk

build-release:
    trunk build --release --features trunk

build-assets: bundle-js build-tailwind bundle-fonts

build-tailwind output-dir="public":
    mkdir -p {{ output-dir }}/css
    cp node_modules/flatpickr/dist/flatpickr.min.css {{ output-dir }}/css/
    npx --yes @tailwindcss/cli -i css/universal-inbox.css -o {{ output-dir }}/css/universal-inbox.min.css --minify

bundle-js:
    npx --yes rspack build

bundle-fonts output-dir="public":
    mkdir -p {{ output-dir }}
    cp -a fonts {{ output-dir }}

clear-dev-assets:
    rm -rf ../target/dx/universal-inbox-web/debug/web/public/assets

## Dev recipes
check: install build-assets
    cargo clippy --tests -- -D warnings

install:
    npm install --dev

## Test recipes
test test-filter="" $RUST_LOG="info": build-assets
    cargo test {{test-filter}}

test-ci: install build-assets
    cargo test

## Run recipes
run: clear-dev-assets build-assets
    #!/bin/bash

    dx serve --port ${DX_SERVE_PORT:-8080} --verbose

run-tailwind output-dir="public":
    cp node_modules/flatpickr/dist/flatpickr.min.css {{ output-dir }}/css/
    npx --yes @tailwindcss/cli -i css/universal-inbox.css -o public/css/universal-inbox.min.css --minify --watch

run-bundle-js:
    npx --yes rspack build --watch

run-trunk:
    trunk serve --features trunk
