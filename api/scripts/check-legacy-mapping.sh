#!/usr/bin/env bash
# Checks that every test of the legacy `api` and `browser` binaries (snapshot in
# tests/features/legacy-tests.txt) is still a legacy test, a scenario tagged
# `@legacy:<test>` (one expanded scenario per legacy case), or one of the tests kept as plain
# Rust tests (tests/features/legacy-kept-as-rust.txt).
set -Eeuo pipefail

cd "$(dirname "$0")/.."
features_dir=tests/features
snapshot="$features_dir/legacy-tests.txt"
kept="$features_dir/legacy-kept-as-rust.txt"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cargo nextest list --message-format json >"$work/list.json"

jq -r '."rust-suites" | to_entries[]
       | select(.value."binary-name" == "api" or .value."binary-name" == "browser")
       | .value."binary-name" as $binary
       | .value.testcases | keys[] | "\($binary)\t\(.)"' \
  "$work/list.json" | sort >"$work/present.txt"

: >"$work/tags.txt"
for binary in api_features browser_features; do
  path=$(jq -r --arg binary "$binary" '."rust-suites" | to_entries[]
           | select(.value."binary-name" == $binary) | .value."binary-path"' "$work/list.json")
  if [ -n "$path" ]; then
    UI_SCENARIOS_LEGACY_REPORT=1 "$path" | cut -f1 >>"$work/tags.txt"
  fi
done
sort "$work/tags.txt" | uniq -c | awk '{ print $2 "\t" $1 }' >"$work/tag_counts.txt"

awk -F'\t' '
  FILENAME == ARGV[1] { present[$1 "\t" $2] = 1; next }
  FILENAME == ARGV[2] { if ($0 != "" && $0 !~ /^#/) kept[$0] = 1; next }
  FILENAME == ARGV[3] { tags[$1] = $2; next }
  {
    total++
    key = $1 "\t" $2
    if (key in present) { still++; next }
    if ($2 in kept) { plain++; next }
    matched = ""
    for (tag in tags) {
      if ($2 == tag || index($2, tag "::") == 1) {
        if (matched != "") { printf "%s matches both @legacy:%s and @legacy:%s\n", $2, matched, tag; errors++ }
        matched = tag
      }
    }
    if (matched == "") { printf "%s (%s) is neither a test anymore nor a tagged scenario\n", $2, $1; errors++; next }
    cases[matched]++
    migrated++
  }
  END {
    for (tag in tags) {
      if (cases[tag] != tags[tag]) {
        printf "@legacy:%s tags %d scenario(s) for %d legacy case(s)\n", tag, tags[tag], cases[tag] + 0
        errors++
      }
    }
    printf "Legacy tests: %d = %d still legacy + %d migrated to scenarios + %d kept as plain Rust tests\n", total, still, migrated, plain
    if (still + migrated + plain != total) errors++
    exit errors > 0
  }
' "$work/present.txt" "$kept" "$work/tag_counts.txt" "$snapshot"
