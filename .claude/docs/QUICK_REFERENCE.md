# Quick Reference

> Fast lookups. See [`INDEX.md`](INDEX.md) for full navigation.

## Session-Start Checklist

1. Read `CLAUDE.md` (auto-loaded) + 3 session-start docs: `.claude/COMMON_MISTAKES.md`,
   `.claude/QUICK_START.md`, `.claude/ARCHITECTURE_MAP.md`.
2. Pick task-specific docs from [`INDEX.md`](INDEX.md) (~500–800 each).
3. Check `bd ready` for available work; `bd update <id> --claim` before starting.

## Common Commands

```bash
just test "name" debug      # debug one test
just check                  # lint + compile (domain crate ONLY)
just check-all              # + `just web check` + `just api check` + PII-logging check
just run-detached           # agents: pg + redis headless (never bare `just run`: needs a TTY)
just api ensure-db          # DB + migrations
just start ui-api           # start a service   |   just logs ui-api | just stop ui-api
just print-env-info         # ports/URLs
cd api && just test         # API tests (nextest, excludes browser)  |  just api test-browser
bd ready | bd show <id> | bd close <id>
```

## Code Patterns

```rust
// Repository method
async fn get_one_task(
    &self,
    executor: &mut Transaction<'_, Postgres>,
    id: TaskId,
) -> Result<Option<Task>, UniversalInboxError>;

// Error chaining
self.repository.get_one_task(executor, id).await
    .context("Failed to fetch task from repository")?;

// Tracing on public service methods
#[tracing::instrument(level = "debug", skip_all, fields(user_id = %user_id))]
pub async fn update_task(/* ... */) -> Result<UpdateStatus<Task>, UniversalInboxError> { /* ... */ }

// rstest test skeleton
#[rstest]
#[tokio::test]
async fn test_x(#[future] tested_app: TestedApp) {
    let app = tested_app.await;
    // ...
}
```

```rust
// Dioxus 0.7 UI component (web/src/components/ui/) — never hardcode colors/sizes
rsx! { Button { variant: ButtonVariant::Primary, "Save" } }
```

## Debugging Tips

- Failing test: `just test "name" debug` for trace logs.
- Stack overflow in tests → `unset RUST_MIN_STACK` (a smaller exported value overrides the `.cargo/config.toml` one).
- SQLx compile error offline → `cd api && just check-db` (runs `cargo sqlx prepare`, refreshes `api/.sqlx/`).
- Commit via `MBX_DISABLE=1 direnv exec . git commit ...`; the pre-commit hook may reformat files (check `git log -1`) and `web/Dioxus.toml` port rewrite must be reverted first.
- Shell `rg` may be aliased to BSD grep: use `command rg`.
- API up? `curl http://localhost:$API_PORT/ping` → `{"cache":"healthy","database":"healthy"}` (per-IP rate limited).
- Service logs: `just logs ui-api` / `just status`.

## Where Things Live

| Need | Path |
|------|------|
| HTTP endpoints | `api/src/routes/` |
| Business logic | `api/src/universal_inbox/` |
| DB layer | `api/src/repository/` |
| Third-party clients | `api/src/integrations/` |
| Background jobs | `api/src/jobs/` |
| Domain types | `src/` |
| UI pages / API clients | `web/src/pages/` · `web/src/services/` |
| Design-system components | `web/src/components/ui/` |
| Migrations | `api/migrations/` (SQLx cache: `api/.sqlx/`) |
| Integration tests | `api/tests/` |

_Last updated: 2026-10-15_
