#!/usr/bin/env bash
set -euo pipefail

data_dir=$(mise daemons ls --json | jq -r '.[] | select(.name == "postgresql") | .data_dir')
conf="$data_dir/postgresql.universal-inbox.conf"

# Tuning for a *disposable* cluster. This one holds nothing but development and test data,
# so trading crash durability for speed is free: `fsync=off` alone removes one disk flush per
# commit, and the test suite commits constantly. Never apply this to a cluster whose contents
# you would miss. Written to a separate file and `include`d so `postgresql.conf` stays the
# stock one initdb produced.
cat > "$conf" <<'CONF'
fsync = off
synchronous_commit = off
full_page_writes = off
max_wal_size = 4GB
checkpoint_timeout = 3600
# Each test process opens its own pool, so the cluster sees test-threads x max_connections.
max_connections = 200
shared_buffers = 256MB
CONF

grep -q "postgresql.universal-inbox.conf" "$data_dir/postgresql.conf" \
  || echo "include = 'postgresql.universal-inbox.conf'" >> "$data_dir/postgresql.conf"
