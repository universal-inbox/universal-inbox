# Common Mistakes — READ FIRST (auto-loaded at session start)

> CRITICAL. These five bite every session on this all-Rust stack. Scan before you build, test, or run anything.

## 1. Tests overflow the stack (missing RUST_MIN_STACK)
- **Symptom:** Tests crash with `thread 'main' has overflowed its stack` / SIGABRT on big nested structs.
- **Check:** `echo $RUST_MIN_STACK` — empty or not `104857600`?
- **Fix:** `export RUST_MIN_STACK=104857600` before tests. `just test` already sets it; don't bypass it with raw `cargo test`.

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

## 5. Frontend styling drift (hardcoded values / stray CSS)
- **Symptom:** `bg-[#388fef]`, inline px radii, or a new `.foo-bar` class in `universal-inbox.css`.
- **Check:** Does a `@theme` token, FlyonUI class, or `web/src/components/ui/` component already cover it?
- **Fix:** Use `bg-ui-primary`, `rounded-ui-md`, `shadow-ui-sm`, `font-ui` — add a token before inlining. Custom CSS only for pseudo-elements/keyframes/sibling cascades/scrollbars.

## Before you finish
- Run `just check` and `just test` from the project you touched (api/web/root).
- Session close is MANDATORY: `git pull --rebase` -> `bd dolt push` -> `git push` -> confirm `up to date with origin`. Work isn't done until pushed.

For the longer list, see [common pitfalls](docs/learnings/common-pitfalls.md).

_Last updated: 2026-05-31_
