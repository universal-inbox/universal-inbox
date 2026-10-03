# Quick Start

> Auto-loaded at session start. Build/test/service commands, per-worktree ports, browser tests.

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

`just run` / `just run-detached` merge three configs into one process-compose instance on
`$PROCESS_COMPOSE_PORT` (default 9999): `.devbox/virtenv/redis/process-compose.yaml` (redis) +
`process-compose-pg.yaml` (postgresql via `pg_ctl`) + `process-compose.yaml` (app services).
`postgresql` + `redis` start automatically; app services `ui-api` `ui-workers` `ui-web`
`build-tailwind` `bundle-js` `caddy` `stripe-listen` are `disabled: true` — start them
explicitly. `stripe-listen` (`just api stripe-listen`) forwards Stripe webhooks to the local
API; it is only useful with `[application.billing]` configured.

```bash
just run               # interactive TUI — HUMANS ONLY, errors in a no-TTY/agent shell
just run-detached      # AGENTS: start pg+redis detached/headless (no TUI); idempotent
just down              # stop the detached stack started by run-detached
just start ui-api      # start one service: ui-api | ui-workers | ui-web | postgresql | redis
just stop <svc>        # stop one service
just status            # service states (✅ running / ⏸️ disabled / ❌ down)
just logs <svc>        # tail -f last 100 lines of a service
just print-env-info    # URLs/ports for web, API, Postgres, Redis
```

Always run service/test commands through `direnv exec .` (or inside a direnv-allowed shell) so
the worktree's ports load — see **Worktree** below. `just run-detached` is the headless
equivalent of `just run`; agents must never call bare `just run` (TUI needs a TTY).

## Browser tests (`api/tests/browser/`)

Need **only postgresql + redis** up (`just run-detached`) — the `browser_tested_app` fixture
spawns its own API in-process and serves the prebuilt WASM frontend from `web/public/`.

```bash
just run-detached                              # pg + redis (once)
just api test-browser                          # builds web/public/ then runs ALL browser tests
just api test-browser test_user_can_register   # ...filter to one test (substring)
```

`just api test-browser` rebuilds the frontend (`just web build-ci`) before running — required
because the test reads `web/public/` (gitignored), not your `.rs` source. Don't call
`cargo nextest` directly: from the workspace root the root `.config/nextest.toml` filters out
`binary(browser)` (0 tests), and you'd skip the frontend rebuild.

## Committing — pre-commit hook needs Postgres + Redis

The pre-commit hook (`.pre-commit-config.yaml`) runs `just format`, `just check` and `just test`
when `.rs` files are staged, then the root `test-browser` hook (`just api test-browser`, which
rebuilds the frontend first) when `api/` `.rs` files are staged. Both include integration tests,
so **Postgres and Redis must be running or the commit fails**.

```bash
direnv exec . just run-detached            # pg + redis (idempotent)
direnv exec . git commit -m "..."          # hook uses this worktree's ports
```

Don't skip the hook with `--no-verify`.

## Test Data

```bash
just api generate-user   # seeds test+{uuid}@test.com / test123456 with sample data
```

## Worktree (worktrunk) — per-worktree ports

```bash
wt switch --create my-feature --yes   # isolated ports, written into .local_envrc
direnv exec . <command>               # REQUIRED: run with the worktree env loaded
```

Each worktree gets **its own ports**, NOT 5432/6379. On create, worktrunk
(`.config/wt.toml` pre-start hook) appends branch-hashed ports to `.local_envrc`:
`PGPORT` `REDIS_PORT` `DX_SERVE_PORT` `API_PORT` `PROCESS_COMPOSE_PORT`, plus the matching
`DATABASE_URL`, `UNIVERSAL_INBOX__{DATABASE__PORT,REDIS__PORT,APPLICATION__*}`, and `PGHOST=/tmp`.
`.envrc` sources `.local_envrc` **last**, so these override the 5432/6379 defaults in `.envrc`.

- **`.local_envrc` is the source of truth for this worktree's ports** — read it (or
  `just print-env-info`) to learn where pg/redis/API/web live.
- **Always prefix with `direnv exec .`.** Bare `devbox run` does NOT source `.local_envrc`,
  so it silently falls back to ports 5432/6379 — wrong DB, and it collides with any local
  Docker postgres on 5432.
- **Stop the branch's services before `wt remove` or `wt merge`.** Run from the worktree:
  `direnv exec . just down` (stops pg, redis and app services on this worktree's
  `PROCESS_COMPOSE_PORT`), then `direnv exec . just status` to confirm nothing runs. The
  `.config/wt.toml` pre-remove hook is only a best-effort fallback (`|| true`); don't rely on
  it. Leftover processes keep ports and the removed worktree's data dir busy.

## Required Env

```bash
RUST_MIN_STACK=104857600   # tests (large nested structs); default set by .cargo/config.toml [env]
SQLX_OFFLINE=true          # use cached query metadata
# DATABASE_URL: 5432 default in .envrc, OVERRIDDEN per worktree by .local_envrc (PGPORT).
# Don't hardcode 5432 — read $DATABASE_URL / $PGPORT from direnv.
DATABASE_URL=postgres://postgres:password@127.0.0.1:${PGPORT:-5432}/universal-inbox
```

Billing is optional and off unless configured. With no `[application.billing]` block every
`/api/billing/*` route 404s and the Free-plan caps are not enforced — that is the self-hosting
default. To work on billing, set `stripe_secret_key`, `stripe_publishable_key`,
`stripe_webhook_signing_secret` and `stripe_price_id` (plus the `[application.billing.free_plan]`
limits) in `api/config/local.toml`; see `api/config/default.toml` for the shape.

_Last updated: 2026-05-31_
