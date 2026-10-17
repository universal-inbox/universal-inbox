# Testing Patterns

Universal Inbox testing conventions. Fast-path learnings doc; see also `.claude/COMMON_MISTAKES.md`.

## Test structure: rstest + tokio

Every async test uses `#[rstest]` + `#[tokio::test]` and takes the `tested_app` fixture as a `#[future]` argument:

```rust
#[rstest]
#[tokio::test]
async fn test_health_check_works(#[future] tested_app: TestedApp) {
    let app = tested_app.await;
    let response = reqwest::Client::new()
        .get(format!("{}/ping", app.app_address))
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
}
```

- `#[future]` makes the fixture async — `.await` it inside the test body.
- `#[case(...)]` keeps integration cases table-driven instead of copy-pasted.
- Use `pretty_assertions::assert_eq` for readable struct/collection diffs.

## TestedApp fixture

`tested_app` (`api/tests/api/helpers/mod.rs`) boots a full app against an **isolated test
database** and exposes `app_address`, the services/repository, and one wiremock mock server per
integration (`github_mock_server`, `notion_mock_server`, `slack_mock_server`, ...). It is the single
entry point for API tests — don't hand-roll a server or use a shared DB.

- The mock servers are `LazyMockServer`s (`api/tests/common/mod.rs`): `uri()` is known up front but
  the server only starts on first use (`mount`, `&*app.x_mock_server`...), which saves a TCP
  connection per unused server. A call to a server nobody started hangs instead of getting a 404.
  `create_integration_connection` starts the connected provider's server
  (`TestedApp::start_mock_server`); for any other call, touch the server in the test setup.

- Variants exist for special setups: `tested_app_with_local_auth`, `..._with_domain_blacklist`,
  `..._with_short_reauthentication_window`, `..._with_account_rate_limits`.
- The default `tested_app` has no local auth, `helpers::create_user` fails ("Cannot hash password");
  for a second user use `authenticate_user(&app.app, provider_id, first, last, email)`.

## Mocks and fixtures

Drive third-party responses from JSON fixtures in `api/tests/api/fixtures/` through the per-provider
helpers in `api/tests/api/helpers/` (`notion.rs`, `slack.rs`, `notification/github.rs`, ...), e.g.
`mock_github_notifications_service(&app.github_mock_server, page, &result)`. Load fixtures with
`load_json_fixture_file` / `fixture_path`.

## Audit actor is mandatory

`api/config/test.toml` sets `application.audit_log.strict_actor = true`: a notification/task write
without an audit actor **fails the test**. Tests calling job handlers, services or commands directly
(including browser-test helpers) must wrap them in `universal_inbox::audit::actor_context::with_actor(actor, fut)`
or call `helpers::set_audit_actor(&mut tx, &actor)` on a transaction they began.

## Stack size and SQLx

`RUST_MIN_STACK=104857600` (100 MiB) is set by `.cargo/config.toml` `[env]`, so no `.envrc` is needed
for tests. Tests deserialize large nested structs; only a smaller *exported* value overflows.
`SQLX_OFFLINE=true` comes from `.envrc`.

## Running tests

Tests run with **cargo nextest** (one process per test). Root `just test` runs **only** the domain crate.

```bash
just test [pattern] [debug]        # root domain crate
cd api && just test [pattern]      # API tests: nextest -E 'not binary(browser)'
cd api && just test-browser        # Playwright browser tests (binary(browser))
cd api && cargo nextest run --lib  # API unit tests (MCP tool-count / schema tests); pre-commit runs these too
cd web && just test                # web (Dioxus/WASM)
```

API tests need Postgres + Redis up (`direnv exec . just run-detached`; prefix `direnv exec .` if env is missing).
`test_mcp_session_store` can time out on Redis under full-suite load: rerun it alone before suspecting code.

## Speed

- One process per test means once-per-process init is paid per test. `api/config/test.toml` caps
  `application.http_workers = 2` and `database.max_connections = 5`, and `governor` is built without
  default features (skips ~200 ms quanta calibration). Don't undo these.
- Near 30 s wall on macOS you can exhaust ephemeral ports ("Can't assign requested address"). A test
  leaves ~5 sockets in TIME_WAIT (Postgres, Redis), plus one per mock server it uses and one per HTTP connection it
  opens. `api/scripts/count-time-wait.sh '<filterset>'` measures it.
- Browser tests are reserved for the most important flows; prefer API tests. Never navigate a browser
  page to `/api/*` (the middleware forces `Content-Disposition: attachment`).

## Where tests live

- **API integration tests:** `api/tests/api/` (`test_*.rs`, one file per feature/integration); **browser:** `api/tests/browser/`.
- **Unit tests:** inline `#[cfg(test)]` modules next to the code under test.

See also: [integrations](integrations.md), [database patterns](database-patterns.md), [common pitfalls](common-pitfalls.md).

_Last updated: 2026-10-17_
