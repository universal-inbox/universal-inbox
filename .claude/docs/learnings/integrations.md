# Integrations

How third-party sources (GitHub, Linear, TickTick, Notion, Slack, Todoist, Google Mail/Calendar/Drive)
plug into Universal Inbox.

## Layout — `api/src/integrations/`

```
integrations/
├── github/            # mod.rs, notification.rs, oauth.rs, graphql/
├── linear/            # mod.rs, oauth.rs, graphql/
├── ticktick/          # mod.rs, oauth.rs (tasks)
├── notion/            # mod.rs, oauth.rs, webhook.rs (comment + mention notifications)
├── oauth2/            # generic OAuth2 provider (mod.rs, provider.rs)
├── slack.rs           # Slack source + slack_oauth.rs (OAuth flow)
├── todoist.rs         # Todoist source + todoist_oauth.rs
├── google_mail.rs     # + google_oauth.rs (shared Google OAuth), icalendar.rs
├── google_calendar.rs
├── google_drive.rs
├── api.rs             # shared HTTP client helpers / pagination
└── mock.rs            # mock project-name constants only (not server scaffolding)
```

Larger providers are **directories** (`oauth.rs` inside); simpler ones are single `.rs`
files with a sibling `*_oauth.rs` (Slack, Todoist). `google_oauth.rs` serves all three Google
services.

## The two source traits

Every integration implements one (or both) of these — they are the contract that lets a
generic sync pipeline pull items without knowing the provider:

Defined in `api/src/integrations/mod.rs`:

- **`ThirdPartyItemSourceService<T>`** — `fetch_items(executor, user_id, last_sync_completed_at)`
  returns `ThirdPartyItem`s (Todoist, TickTick, Linear issues).
- **`ThirdPartyNotificationSourceService<T>`** (extends `NotificationSource`) — turns third-party
  items into notifications via `third_party_item_into_notification` (GitHub, Slack, Google, Linear, Notion).
- `ThirdPartyTaskService<T>` / `ThirdPartyTaskSourceService<T>` cover task actions and task sources.

Methods are async and take `&mut Transaction<'_, Postgres>`. Mapping into the domain
`Notification` / `Task` happens in `NotificationService` / `TaskService`, which own the upsert
(`UpsertStatus<T>`). Decide the final status **before** upserting, or a re-sync can flip it
(see the Linear auto-deleted fix).

## Adding a new integration — 7-step checklist

1. **Service** — create `api/src/integrations/new_service.rs` (or a dir for bigger ones)
   with the HTTP/GraphQL client and types.
2. **Trait impl** — implement `ThirdPartyItemSourceService` or
   `ThirdPartyNotificationSourceService` on it.
3. **Repository methods** — add any new queries in `api/src/repository/`.
4. **Routes** — add HTTP handlers in `api/src/routes/` (webhook endpoint goes in
   `routes/webhook.rs`, OAuth callbacks in `routes/oauth.rs` / `routes/oauth2.rs`); keep business
   logic out of handlers.
5. **Wire it in** — register the service on `NotificationService` and/or `TaskService`
   (held as `Arc<Service>`; `Weak<RwLock<...>>` for circular refs).
6. **Migration** — add `api/migrations/YYYYMMDDHHMMSS_description.{up,down}.sql`
   (run `just api migrate-db`).
7. **Tests** — add integration tests in `api/tests/api/` with a mock server (below).

## OAuth2 flow (`*_oauth.rs` / `oauth.rs`)

`oauth2/` holds the generic provider machinery; each integration's oauth file wires its client
id/secret, scopes and token endpoint. Tokens are stored per user via `integration_connection`.
Callbacks land in `routes/oauth.rs` and `routes/oauth2.rs`.

- **Google scopes come in pairs:** the Calendar callback reads the account email via `calendars.get`
  on `/calendars/primary`, which needs `calendar.calendars.readonly` beside `calendar.events`;
  narrowing one without the other gives 403 on connect.
- Never put `provider_user_id` in span fields: for Google it is the account email.
- After connect/disconnect call `invalidate_cached_provider_lookups`, or cached webhook lookups go stale.

## Background sync — Apalis jobs (`api/src/jobs/`)

Syncing runs as Apalis (Redis-backed) jobs, never inline on a request: `sync.rs`, `cron.rs`,
`oauth.rs`, `slack/`, `notion/`. Handlers call the source service, then the notification/task
service to upsert. Workers must be running (`just run` starts the full stack; or `just run-detached` then `just start ui-workers`).
Webhook routes (Slack, Notion) verify the signature, then enqueue a job (e.g. `NotionWebhookEventJob`)
that runs under `AuditActor::Webhook`. A Slack API error mapped to a normal outcome (`bot_not_found`,
`user_not_found`) must not mark the span failed.

```bash
cd api && just clear-cache   # flush Redis (cache + queue) if jobs get stuck
```

## Mocking in tests

- `TestedApp` (`api/tests/api/helpers/mod.rs`) holds one wiremock `MockServer` per integration
  (`github_mock_server`, `notion_mock_server`, ...), so no real API is hit.
- Mock helpers live in `api/tests/api/helpers/` (`notion.rs`, `slack.rs`, `notification/github.rs`, ...);
  JSON fixtures in `api/tests/api/fixtures/`. Running tests: [testing patterns](testing-patterns.md).

## Gotchas

- A new source must be wired into **both** the service registration (step 5) **and** a job
  handler (`api/src/jobs/`) — implementing the trait alone won't make sync happen.
- `mock_slack_fetch_thread` indexes `messages[0]`, so it cannot serve a Slack error body; mock
  `/conversations.replies` directly for error cases.
- `SQLX_OFFLINE=true` (from `.envrc`) uses cached query metadata; after changing SQL, regenerate it
  or unset offline mode, or compilation fails on the new query.
- Map domain types in `NotificationService` / `TaskService`, not in the integration client —
  keep the client returning raw third-party shapes.

See also: [database patterns](database-patterns.md), [testing patterns](testing-patterns.md),
[API design](api-design.md).

_Last updated: 2026-10-15_
