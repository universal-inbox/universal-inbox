# Quick Start

> Auto-loaded at session start. Full worktree + Playwright flow in [`AGENTS.md`](../AGENTS.md).

## Build & Test (run from the relevant project dir)

```bash
just test                  # all tests (root crate)
just test "pattern"        # filter by name
just test "pattern" debug  # filter + debug logs (best for debugging)
just check                 # lint + compile checks
just build                 # build
just format                # format
cd api && just test        # API tests   |   cd web && just test   # web tests
just check-all | just test-all   # whole workspace
```

## Database

```bash
just api ensure-db    # create DB if missing + run SQLx migrations
just api migrate-db   # run migrations
just api clear-cache  # flush Redis
just connect-to-db    # psql into the DB
cd api && just check-db
```

## Dev Servers (process-compose)

```bash
just run               # TUI (humans only — fails in non-interactive shells)
just start ui-api      # ui-api | ui-workers | ui-web
just status            # service states
just logs <svc>        # tail a service
just print-env-info    # URLs/ports for web, API, Postgres, Redis
```

Non-interactive shells: start process-compose headless (see `AGENTS.md`). Read `$API_PORT`,
`$DX_SERVE_PORT`, `$PROCESS_COMPOSE_PORT` from direnv — they vary per worktree.

## Test Data

```bash
just api generate-user   # seeds test+{uuid}@test.com / test123456 with sample data
```

## Worktree (worktrunk)

```bash
wt switch --create my-feature --yes   # isolated ports via direnv
direnv exec . <command>               # run with the worktree env loaded
```

## Required Env

```bash
RUST_MIN_STACK=104857600   # tests (large nested structs); omit => stack overflow
SQLX_OFFLINE=true          # use cached query metadata
DATABASE_URL=postgres://postgres:password@127.0.0.1:5432/universal-inbox
```

_Last updated: 2026-05-30_
