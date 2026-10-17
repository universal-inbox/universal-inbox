#!/usr/bin/env bash
# Counts the TCP sockets an API test run leaves in TIME_WAIT, split by peer:
# Postgres, Redis and everything else (test HTTP client, wiremock readiness
# probes, mock server calls) on loopback. Each one held an ephemeral port that macOS only
# frees after 2 x net.inet.tcp.msl (30 s by default), see .claude/QUICK_START.md.
#
# Usage (from api/, with pg + redis up): direnv exec . scripts/count-time-wait.sh [filterset]
#   scripts/count-time-wait.sh 'test(=test_health_check::health_check_works)'
# The filterset defaults to the whole API suite, 'not binary(browser)'.
#
# Keep the run shorter than the TIME_WAIT duration, or the first sockets expire
# before the count: the result is then a lower bound. Other processes on the
# machine (another worktree's tests) also show up in "other".
set -euo pipefail

pg_port=$(echo "$DATABASE_URL" | sed -E 's|.*:([0-9]+)/.*|\1|')
redis_port=${UNIVERSAL_INBOX__REDIS__PORT:-${REDIS_PORT:?REDIS_PORT is not set}}

# Prints "<postgres> <redis> <other>" TIME_WAIT counts.
count() {
  netstat -an -p tcp | awk -v pg="\\.${pg_port}\$" -v redis="\\.${redis_port}\$" '
    $NF == "TIME_WAIT" {
      if ($4 ~ pg || $5 ~ pg) p++
      else if ($4 ~ redis || $5 ~ redis) r++
      else if ($4 ~ /^127\.0\.0\.1\./) o++
    }
    END { printf "%d %d %d\n", p, r, o }'
}

filter=${1:-not binary(browser)}

cargo nextest run --no-run -E "$filter" >/dev/null 2>&1

read -r pg_before redis_before other_before < <(count)
start=$(date +%s)
cargo nextest run --no-fail-fast -E "$filter" 2>&1 | grep -E '^\s+Summary' || true
elapsed=$(($(date +%s) - start))
read -r pg_after redis_after other_after < <(count)

pg=$((pg_after - pg_before))
redis=$((redis_after - redis_before))
other=$((other_after - other_before))
echo "TIME_WAIT delta after ${elapsed}s (msl $(sysctl -n net.inet.tcp.msl) ms):"
echo "  postgres: ${pg}"
echo "  redis:    ${redis}"
echo "  other:    ${other}"
echo "  total:    $((pg + redis + other))"
