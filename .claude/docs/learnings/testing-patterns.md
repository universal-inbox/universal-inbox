# Testing Patterns

Universal Inbox testing conventions. Fast-path learnings doc; see also `.claude/COMMON_MISTAKES.md`.

## Scenarios (cucumber-rs)

API integration tests and browser tests are Gherkin scenarios run by cucumber-rs. A scenario is a
nextest test (one process per scenario), named `<feature file>::<Rule>::<Scenario>`; a Scenario
Outline gives one test per Examples row.

- Features: `api/tests/features/<domain>/*.feature` (API and browser).
- API binary `api_features` (`api/tests/api_features/`): `steps/*.rs` (register in `steps/mod.rs`),
  `state/*.rs` (what a scenario remembers between steps, a field of `ApiWorld` in `world.rs`).
- Browser binary `browser_features` (`api/tests/browser_features/`), Playwright.
- Steps speak the user's language and are shared: parameterise with typed parameters
  (`impl Parameter` + `FromStr`, e.g. `{task_state}`, `{stripe_status}`), never one step per scenario.
  Two steps with the same text in two modules fail as ambiguous: search before writing one.
- A step that the feature uses after `Then` with `And` is a `Then`: stack `#[given]`/`#[when]`/`#[then]`
  on one function when a sentence is used in several positions.
- The app is started per scenario from `AppConfig` (`api/tests/common/scenarios.rs`); a `Given` can
  change it before the first use (`world.app.configure()`): local auth, blocked domains, billing
  with the real Stripe client and a Stripe mock (`billing with Stripe is enabled`).

```gherkin
Scenario: Another user cannot delete my notification
  Given I have the notification "mine" of the Google Mail thread 123
  And another user is signed in
  And that other user has their own Google Mail connection, and their own notification "theirs" of the same thread
  And Google Mail is not asked anything
  When that other user tries to change the status of "mine" to deleted
  Then the app answers 403
  And "mine", read again, is exactly as it was
  And the notification "theirs" of that other user, read by that other user, is exactly as it was
```

## Rules enforced in review

- **A second user for every user-scoped route or job.** The other user has their own connection,
  their own token and their own object of the same external id (same thread, same item, same
  customer). Assert the exact refusal, then read both users' objects back unchanged.
- **A deterministic twin for every zero.** "Nothing happened" needs a scenario on the same fixture
  where the thing does happen, else the zero proves nothing.
- **Pin the calls to third parties.** Mocks match the bearer token and the body and `expect(1)`;
  where nothing must be asked, mount a catch-all `any()` with `expect(0)` after the specific mocks
  (wiremock answers with the first mounted match).
- **Assert what is stored.** The answer of a `PATCH`/`POST` is not its effect: read the object again
  and compare it whole (`assert_the_patched_notification_is_stored`, `assert_the_task_is_stored`).
  Compare whole structs or tuples, not one field at a time.
- **Wait for jobs, do not sleep.** `the background jobs are all done` (fails on a dead or retrying
  job) or `the background jobs have all ended` plus the exact counts when a job is expected to
  fail (`api/tests/common/helpers/jobs.rs`). The test queue does not re-run waiting retries.
- **Exact errors.** Assert the status and the message, never "any 4xx".
- **Mutate before reporting.** Break the code the scenario covers, see the scenario go red on the
  assertion meant for it, restore, and check `git diff -- src` is empty. SQL in `sqlx::query!`
  cannot be mutated offline: say so instead of claiming a kill.
- **Step code:** no boolean flags (enums or two functions), no raw SQL in steps (a helper under
  `api/tests/common/helpers/`), no comments except a forced value or a known quirk.

## Plain Rust tests

Seven tests stay plain rstest in the `plain` binary (`api/tests/plain/`): cron settings, the cache
round trip and a fixture shape. Unit tests live inline in `#[cfg(test)]` modules next to the code.

## Helpers, mocks and fixtures

`TestedApp` (`api/tests/common/helpers/mod.rs`) boots a full app against an **isolated test
database** and exposes the services, the repository and one wiremock server per integration
(`github_mock_server`, `notion_mock_server`, ...).

- The mock servers are `LazyMockServer`s (`api/tests/common/mod.rs`): `uri()` is known up front but
  the server only starts on first use (`mount`, `&*app.x_mock_server`...), which saves a TCP
  connection per unused server. A call to a server nobody started hangs instead of getting a 404.
  `create_integration_connection` starts the connected provider's server.
- Per-provider mock helpers: `api/tests/common/helpers/` (`notion.rs`, `slack.rs`,
  `notification/github.rs`, `task/todoist.rs`, ...). JSON fixtures: `api/tests/api/fixtures/`
  (`load_json_fixture_file`); static files: `api/tests/common/statics/`.
- A second user: the step `another user is signed in` (identity provider), or
  `authenticate_user(app, provider_id, first, last, email)` for more.

## Audit actor is mandatory

`api/config/test.toml` sets `application.audit_log.strict_actor = true`: a notification/task write
without an audit actor **fails the test**. Tests calling job handlers, services or commands directly
(including browser-test helpers) must wrap them in `universal_inbox::audit::actor_context::with_actor(actor, fut)`
or call `helpers::set_audit_actor(&mut tx, &actor)` on a transaction they began.

## Stack size and SQLx

`RUST_MIN_STACK=104857600` (100 MiB) is set by `.cargo/config.toml` `[env]`, so no `mise.toml` env is needed
for tests. Tests deserialize large nested structs; only a smaller *exported* value overflows.
`SQLX_OFFLINE=true` comes from `mise.toml`.

## Running tests

Tests run with **cargo nextest** (one process per test). Root `just test` runs **only** the domain crate.

```bash
just test [pattern] [debug]        # root domain crate
cd api && just test [pattern]      # scenarios + plain tests: nextest -E 'not binary(browser_features)'
cd api && just test-browser        # Playwright browser scenarios (binary(browser_features))
cd api && cargo nextest run --lib  # API unit tests (MCP tool-count / schema tests); pre-commit runs these too
cd web && just test                # web (Dioxus/WASM)
```

API tests need Postgres + Redis up (`mise exec -- just run-detached`; prefix `mise exec --` if env is missing).
The MCP session store scenarios can time out on Redis under full-suite load: rerun them alone before suspecting code.

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

- **Scenarios:** `api/tests/features/`, steps under `api/tests/api_features/` and `api/tests/browser_features/`.
- **Shared test code:** `api/tests/common/` (harness, helpers, static files); fixtures in `api/tests/api/fixtures/`.
- **Plain Rust tests:** `api/tests/plain/`.
- **Unit tests:** inline `#[cfg(test)]` modules next to the code under test.

See also: [integrations](integrations.md), [database patterns](database-patterns.md), [common pitfalls](common-pitfalls.md).

_Last updated: 2026-11-26_
