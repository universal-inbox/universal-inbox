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

build-ci: install build-assets build

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
    #!/usr/bin/env bash

    # Update Dioxus.toml proxy to use the correct API port
    API_URL="http://localhost:${API_PORT:-8000}/api/"
    sed -i.bak "s|backend = \"http://localhost:[0-9]*/api/\"|backend = \"${API_URL}\"|" Dioxus.toml

    dx serve --port ${DX_SERVE_PORT:-8080} --verbose

run-tailwind output-dir="public":
    cp node_modules/flatpickr/dist/flatpickr.min.css {{ output-dir }}/css/
    npx --yes @tailwindcss/cli -i css/universal-inbox.css -o public/css/universal-inbox.min.css --minify --watch

run-bundle-js:
    npx --yes rspack build --watch

run-trunk:
    trunk serve --features trunk

## Mobile recipes

# Root directory (needed for absolute paths required by dx build from web/ subdir)
root := justfile_directory() / ".."
android_sdk := root / ".devbox/nix/profile/default/share/android-sdk"
android_app_dir := root / "target/dx/universal-inbox-web/debug/android/app"
apk_path := android_app_dir / "app/build/outputs/apk/debug/app-debug.apk"
avd_home := root / ".android/avd"
adb := android_sdk + "/platform-tools/adb"

[private]
mobile-env:
    #!/usr/bin/env bash
    # This recipe is a no-op; it exists to document required env vars.
    # Ensure ANDROID_HOME, ANDROID_NDK_HOME, and JAVA_HOME are set
    # (e.g. via direnv / .envrc) before running mobile recipes.
    true

mobile-serve: build-assets
    dx serve --platform android

mobile-build: build-assets
    #!/usr/bin/env bash
    set -euo pipefail

    # Ensure Android env vars use absolute paths (dx resolves them relative to CWD)
    export ANDROID_HOME="{{ android_sdk }}"
    export ANDROID_SDK_ROOT="$ANDROID_HOME"
    NDK_DIR=$(ls -d "$ANDROID_HOME/ndk/"* 2>/dev/null | head -1)
    export NDK_HOME="$NDK_DIR"
    export ANDROID_NDK_HOME="$NDK_DIR"
    export JAVA_HOME="$(dirname "$(dirname "$(realpath "$(which java)")")")"

    dx build --platform android

    # Workaround for Dioxus 0.7.3 bug: circular typealias crashes Kotlin 2.0.20
    bash "{{ root }}/scripts/fix-dioxus-kotlin.sh" "{{ android_app_dir }}"

    "{{ android_app_dir }}/gradlew" -p "{{ android_app_dir }}" assembleDebug

mobile-build-release: build-assets
    dx build --platform android --release

mobile-create-avd:
    #!/usr/bin/env bash
    set -euo pipefail
    export ANDROID_AVD_HOME="{{ avd_home }}"
    mkdir -p "$ANDROID_AVD_HOME"
    if "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" list avd 2>/dev/null | grep -q "universal_inbox_test"; then
        echo "AVD 'universal_inbox_test' already exists"
    else
        echo "no" | "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" create avd \
            --name universal_inbox_test \
            --package "system-images;android-33;google_apis;arm64-v8a" \
            --device pixel_6
        echo "AVD 'universal_inbox_test' created"
    fi

mobile-emulator headless="false": mobile-create-avd
    #!/usr/bin/env bash
    set -euo pipefail
    export ANDROID_AVD_HOME="{{ avd_home }}"
    if "{{ adb }}" devices 2>/dev/null | grep -q "emulator-"; then
        echo "Emulator already running"
        exit 0
    fi
    EXTRA_ARGS=()
    if [ "{{ headless }}" = "true" ]; then
        EXTRA_ARGS+=(-no-window -no-audio -gpu swiftshader_indirect)
    fi
    echo "Starting emulator..."
    "$ANDROID_HOME/emulator/emulator" -avd universal_inbox_test \
        "${EXTRA_ARGS[@]}" &
    "{{ adb }}" wait-for-device shell 'while [[ -z $(getprop sys.boot_completed) ]]; do sleep 1; done'
    echo "Emulator ready"

mobile-deploy: mobile-build mobile-emulator
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Installing APK..."
    "{{ adb }}" install -r "{{ apk_path }}"
    echo "Launching app..."
    "{{ adb }}" shell am force-stop com.universalinbox.app
    sleep 1
    "{{ adb }}" shell am start -n "com.universalinbox.app/dev.dioxus.main.MainActivity"
    echo "App launched on emulator"

mobile-logcat:
    #!/usr/bin/env bash
    set -euo pipefail
    APP_PID=$("{{ adb }}" shell pidof com.universalinbox.app 2>/dev/null || true)
    if [ -n "$APP_PID" ]; then
        "{{ adb }}" logcat --pid="$APP_PID"
    else
        echo "App not running. Showing recent Rust logs..."
        "{{ adb }}" logcat -d -s RustStdoutStderr
    fi

mobile-screenshot file="/tmp/universal-inbox-android.png":
    "{{ adb }}" exec-out screencap -p > {{ file }}
    @echo "Screenshot saved to {{ file }}"

mobile-stop-emulator:
    "{{ adb }}" emu kill 2>/dev/null || true
    @echo "Emulator stopped"
