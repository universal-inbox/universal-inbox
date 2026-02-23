#!/usr/bin/env bash
# Workaround for Dioxus 0.7.3 bug: the generated MainActivity.kt contains
# `typealias BuildConfig = BuildConfig;` which is circular and crashes
# Kotlin 2.0.20 compiler with an internal compiler error (ICE).
#
# This script:
# 1. Renames the circular typealias to a non-conflicting name
# 2. Adds an explicit BuildConfig import to Logger.kt which references it
set -euo pipefail

APP_DIR="${1:?Usage: fix-dioxus-kotlin.sh <android-app-dir>}"
KT_DIR="$APP_DIR/app/src/main/kotlin/dev/dioxus/main"
MAIN_KT="$KT_DIR/MainActivity.kt"
LOGGER_KT="$KT_DIR/Logger.kt"

if [ ! -f "$MAIN_KT" ]; then
    echo "Error: $MAIN_KT not found" >&2
    exit 1
fi

APP_PKG=$(grep 'namespace' "$APP_DIR/app/build.gradle.kts" | sed 's/.*"\(.*\)".*/\1/')

# Fix circular typealias in MainActivity.kt
sed -i.bak 's/^typealias BuildConfig = BuildConfig;/@Suppress("unused") typealias DioxusBuildConfig = BuildConfig;/' "$MAIN_KT"
rm -f "$MAIN_KT.bak"

# Add explicit BuildConfig import to Logger.kt
IMPORT_LINE="import ${APP_PKG}.BuildConfig"
if ! grep -q "$IMPORT_LINE" "$LOGGER_KT"; then
    sed -i.bak "/^import android.util.Log/a\\
$IMPORT_LINE
" "$LOGGER_KT"
    rm -f "$LOGGER_KT.bak"
fi

echo "Patched Kotlin sources for Dioxus 0.7.3 BuildConfig workaround"
