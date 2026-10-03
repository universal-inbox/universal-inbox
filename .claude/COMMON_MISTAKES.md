# Common Mistakes — READ FIRST (auto-loaded at session start)

> CRITICAL. These six bite every session on this all-Rust stack. Scan before you build, test, or run anything.

## 1. Tests overflow the stack (missing RUST_MIN_STACK)
- **Symptom:** Tests crash with `thread 'main' has overflowed its stack` / SIGABRT on big nested structs.
- **Check:** `echo $RUST_MIN_STACK` — exported but smaller than `104857600`? An exported value overrides the default.
- **Fix:** `.cargo/config.toml` `[env]` sets `RUST_MIN_STACK=104857600` for every `cargo test`/`cargo nextest` run, so no export is needed. `unset RUST_MIN_STACK` if a smaller value is exported.

## 2. SQLx compile errors offline (stale query cache)
- **Symptom:** `error: failed to find data for query ...` or `SQLX_OFFLINE=true but ...` after editing a `query!`/`query_as!`.
- **Check:** Did you change SQL without regenerating `.sqlx/`? `cd api && just check-db`.
- **Fix:** With the DB up, run `cd api && cargo sqlx prepare`, then commit the updated `.sqlx/`. `SQLX_OFFLINE=true` reads that cache.

## 3. `just run` hangs/errors in a non-interactive shell
- **Symptom:** `open /dev/tty: device not configured` — process-compose TUI can't attach.
- **Check:** Are you an agent / no TTY? Then never use bare `just run`.
- **Fix:** `direnv exec . just run-detached` (headless pg+redis), then `just start ui-api|ui-workers|ui-web` as needed; tear down with `just down`. See QUICK_START.md "Dev Servers".

## 4. Commands in a worktree hit the wrong DB/ports
- **Symptom:** Connection refused, or you mutate the main checkout's DB from a worktree.
- **Check:** Did you prefix with `direnv exec .`? Per-branch ports live in `.local_envrc` (PGPORT/REDIS_PORT/…, NOT 5432/6379). Bare `devbox run` skips `.local_envrc` → wrong ports.
- **Fix:** `cd /abs/path/to/worktree && direnv exec . just <cmd>`. Shell state doesn't persist across agent calls. See QUICK_START.md "Worktree".
- **Before `wt remove` / `wt merge`:** stop that worktree's services first: `direnv exec . just down`, then `direnv exec . just status` to confirm. Orphaned pg/redis/API processes hold the branch ports.
- **Builds link another worktree's code** (e.g. E0063 on a field your branch has): `readlink target` matches another worktree's? `target` is an mbx symlink and must be per-worktree. Fix: `rm target` (the symlink only), then rebuild. Never `mbx clean` here: it deletes the shared dir.

## 5. Frontend styling drift (hardcoded values / stray CSS)
- **Symptom:** `bg-[#388fef]`, inline px radii, or a new `.foo-bar` class in `universal-inbox.css`.
- **Check:** Does a `@theme` token, FlyonUI class, or `web/src/components/ui/` component already cover it?
- **Fix:** Use `bg-ui-primary`, `rounded-ui-md`, `shadow-ui-sm`, `font-ui` — add a token before inlining. Custom CSS only for pseudo-elements/keyframes/sibling cascades/scrollbars.

## 6. `git commit` fails in the pre-commit hook (services not running)
- **Symptom:** Commit aborts in the `Test Rust code` hook with DB/Redis connection errors (connection refused, pool timeout).
- **Check:** The pre-commit hook (`.pre-commit-config.yaml`) runs `just test` — integration tests included — whenever a `.rs` file is staged, and the root `test-browser` hook (`just api test-browser`) when an `api/` `.rs` file is. Are Postgres and Redis up? `direnv exec . just status`.
- **Fix:** `direnv exec . just run-detached` before committing, then commit through `direnv exec .` so the hook sees this worktree's ports. Never bypass with `--no-verify`.

## Before you finish
- Run `just check` and `just test` from the project you touched (api/web/root).
- Then file follow-up issues and run `bd dolt push`. Push git branches only when the user asks.

For the longer list, see [common pitfalls](docs/learnings/common-pitfalls.md).

_Last updated: 2026-05-31_
