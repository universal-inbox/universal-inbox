# Common Pitfalls

Repo-specific anti-patterns and the traps that bite repeatedly. See [`COMMON_MISTAKES.md`](../../COMMON_MISTAKES.md) for the top six.

## Tests / Build

- **Stack overflow in tests** — a `RUST_MIN_STACK` smaller than `104857600` exported in the
  shell overrides the one `.cargo/config.toml` sets: the big nested domain structs blow the
  default stack. `unset` it. Symptom: `thread '...' has overflowed its stack` / SIGABRT.
- **Editing SQLx queries without re-preparing the cache** — `SQLX_OFFLINE=true` reads cached
  query metadata from `api/.sqlx/`. Change a `query!`/`query_as!` and the build fails or compiles
  against stale schema until you regenerate:
  ```bash
  cd api && just check-db    # cargo sqlx prepare (MBX_DISABLE=1: mbx can delete .sqlx files)
  ```
- **Quality gate only covers the domain crate** — root `just check` skips `web/` and `api/`.
  Run `just check-all` (adds `just web check`, `just api check`, `check-pii-logging`).
- **Migrations not applied** — connecting to a fresh worktree DB without running migrations
  gives "relation does not exist". Run `just api ensure-db` (creates the `universal-inbox`
  DB if missing + runs migrations).

## Running the app

- **`just run` in a non-interactive env** — defaults to the process-compose TUI and dies with
  `open /dev/tty: device not configured`. Agents must start the orchestrator headless (see
  [Quick Start](../../QUICK_START.md)): `just run-detached`, then `just start ui-api`,
  `just start ui-workers`, `just start ui-web` (one service per call).
- **Running commands in a worktree without `direnv exec .`** — the worktree's branch-hashed
  port overrides (`PGPORT`, `API_PORT`, `DX_SERVE_PORT`...) live in `.local_envrc`. Without
  `direnv exec .` you silently hit the default ports (`5432`/`8000`/`8080`) on the main
  checkout. Always wrap: `cd /abs/path/to/worktree && direnv exec . just status`.

## Commits / worktrees

- **Bare `git commit`** — the pre-commit (prek) clippy hook runs outside devbox and fails
  (`clang: invalid linker name -fuse-ld=lld`). Use `MBX_DISABLE=1 direnv exec . git commit ...`.
- **Hook reformats and aborts** — the Rust format hook can modify files and abort; check
  `git log -1`, re-add, commit again.
- **`web/Dioxus.toml` drift** — `just web run` rewrites its `backend` port to `$API_PORT`;
  this tracked change blocks prek ("Hook changes conflicted with the saved unstaged changes").
  Revert it (`git checkout web/Dioxus.toml`) before committing or `wt merge`. Same for `devbox.lock`.
- **Stale mbx WASM** — `test-browser` hook failing on another worktree's `web/package.json`:
  `rm target/wasm32-unknown-unknown/debug/universal-inbox-web.wasm`, commit with `MBX_DISABLE=1`.

## Backend (Actix / SQLx / services)

- **Forgetting `transaction.commit()`** — `service.begin()` then doing work but never
  committing rolls everything back on drop. Writes "succeed" in the handler then vanish.
  Always end with `transaction.commit().await.context("Failed to commit transaction")?`.
- **Circular service refs without `Weak`** — `NotificationService` ↔ `TaskService` reference
  each other. Use `Weak<RwLock<Service>>` for the back-reference; two `Arc`s leak (the cycle
  never drops) and can deadlock construction.
- **Replace vs patch on updates** — full-replace update logic (the `update-tasks` style that
  overwrites every field) destroys data the source still owns — e.g. wiping recurrence on a
  Todoist task. Patch only the fields you intend to change; preserve untouched fields and
  use `UpdateStatus`/`UpsertStatus` rather than blindly clobbering.

- **Logging personal data** — wrap emails/names in `Pii<T>`; never call `.expose()`/`.into_inner()`
  inside `tracing` macros or `#[instrument]` (`just check-pii-logging` fails). Keep secrets and
  OIDC codes out of logs and browser storage.

## Frontend (Dioxus / styling)

- **Stale dev assets** — `ui-web` builds Tailwind once (no `--watch`) and `dx serve` hot-reload is
  unreliable in worktrees: new utility classes or missing rebuilds need
  `process-compose process restart ui-web -p $PROCESS_COMPOSE_PORT`. After `web/js/index.js` edits
  run `cd web && just bundle-js` and touch a `web/src` file. Missing `public/js/index.js` after
  pre-commit: `direnv exec . just check` in `web/`.

- **Hardcoding colors/fonts/radii in component code** — `bg-[#388fef]`, inline hex, raw font
  stacks. Use `@theme` tokens: `bg-ui-primary`, `text-ui-error`, `rounded-ui-md`,
  `shadow-ui-sm`, `font-ui`. Missing token? Add it to the `@theme inline` block, don't inline.
  Dark mode resolves automatically via `@custom-variant dark` — never duplicate rules.
- **Adding custom CSS instead of reusing a component** — check `web/src/components/ui/`
  (`Button`, `Badge`, `Tag`, `Card`, `StatusLeaf`, `ToggleSwitch`, `NavItem`, `Kbd`,
  `BrandTile`, `PageHeader`, `UISelect`, `Modal*`...) before writing anything. Custom CSS in
  `web/css/universal-inbox.css` is the last resort — only pseudo-elements, `@keyframes`,
  sibling cascades, scrollbar, and form-control internals. See
  [frontend patterns](frontend-dioxus.md).

## Related

- [Testing patterns](testing-patterns.md) · [Database patterns](database-patterns.md) ·
  [API design](api-design.md) · [Integrations](integrations.md)

_Last updated: 2026-10-15_
