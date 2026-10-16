# API Design (Actix-web backend)

Backend lives in `api/`. See the [architecture map](../../ARCHITECTURE_MAP.md) for the big picture.

## Route handlers — `api/src/routes/`

HTTP endpoints are thin: parse input, open a transaction, call a service, commit, map errors. Route modules:

`audit_event` · `auth` · `config` · `health_check` · `integration_connection` · `notification` · `oauth` · `oauth2` · `slack_bridge` · `static_files` · `task` · `third_party` · `user` · `webhook` · `well_known` (MCP tools live in `api/src/mcp/`)

No business logic in handlers (enforced in review). A handler parses input, opens/commits a transaction, calls ONE service method, maps the result. Domain rules, security policy and multi-step orchestration belong in a service, which reports outcomes as `UniversalInboxError` variants (mapped in `routes/mod.rs`), never as an `HttpResponse`. Example: the login throttle lives in `UserService::validate_credentials` and surfaces as `TooManyLoginAttempts` -> 429 + `Retry-After`.

## Business logic — `api/src/universal_inbox/`

The real work lives in services (`NotificationService`, `TaskService`, `IntegrationConnectionService`, `UserService`, `AuditService`, ...). All methods are `async`, take a `&mut Transaction<'_, Postgres>`, and return `Result<_, UniversalInboxError>`.

## Service ownership patterns

- `Arc<Service>` for shared ownership across the app.
- `Weak<RwLock<Service>>` to break circular references (e.g. `NotificationService` <-> `TaskService`).
- `RwLock<Service>` for interior mutability when a service must be mutated after wiring.

```rust
pub struct NotificationService {
    pub(super) repository: Arc<Repository>,
    pub github_service: Arc<GithubService>,
    // ... one Arc per provider service ...
    pub(super) task_service: Weak<RwLock<TaskService>>, // Weak — circular ref
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
}
```

## Error -> HTTP status mapping

Return `UniversalInboxError` everywhere; the variant determines the HTTP code. Build error chains with `anyhow::Context`.

| Variant | HTTP status |
|---|---|
| `InvalidInputData`, `InvalidParameters`, `UnsupportedAction`, `ItemNotFound` | 400 |
| `Unauthorized` | 401 |
| `PaymentRequired` | 402 |
| `Forbidden`, `ReauthenticationRequired` (code `reauthentication_required`) | 403 |
| `AlreadyExists`, `Conflict` | 409 |
| `TooManyLoginAttempts`, `TooManyRequests` | 429 + `Retry-After` |
| `EmailDisabled` | 501 |
| `DatabaseUnavailable`, `SessionStoreUnavailable` | 503 + `Retry-After` |
| `UpstreamServiceError { kind }` | per `UpstreamErrorKind` (400/402/429/500) |
| `DatabaseError`, `Unexpected`, `Recoverable`, `OAuth2InvalidGrant`, `InvalidEnumData`, `InvalidUrlData` | 500 |

500/503 bodies are generic and carry a correlation id (trace id); details stay in logs/traces.

```rust
self.repository
    .get_one_task(executor, id)
    .await
    .context("Failed to fetch task from repository")
```

## Tracing on public methods

Instrument every public service method so spans carry the relevant IDs. Use the `attr::` constants from `api/src/observability/attributes.rs`, never string literals; never put PII (e.g. `provider_user_id`, which is the email for Google) in span fields:

```rust
#[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
pub async fn update_task(...) -> Result<UpdateStatus<Task>, UniversalInboxError> { ... }
```

See [observability.md](observability.md) for span boundary rules.

## Input validation — `validator` crate

Validate request payloads at the boundary. Construct via a `try_new` that runs `validate()`:

```rust
#[derive(Deserialize, Serialize, Validate)]
pub struct RegisterUserParameters {
    pub credentials: Credentials,
}

impl RegisterUserParameters {
    pub fn try_new(credentials: Credentials) -> Result<Self, anyhow::Error> {
        let params = Self { credentials };
        params.validate()?;
        Ok(params)
    }
}
```

## Rate limits and audit actor

- Costly authenticated endpoints (sync, bulk PATCH) call `check_user_rate_limit` with `UserRateLimiters` (`api/src/utils/rate_limit.rs`) -> `TooManyRequests`. Register new costly endpoints there.
- Notification/task writes are audited by DB triggers; the actor comes from `audit::actor_context::with_actor(...)`. Jobs, commands and tests calling services directly must wrap the call in `with_actor` (tests run with `strict_actor`).

## Transaction per request

One transaction spans the handler: `begin()` -> service calls -> `commit()`. Add `.context(...)` to each step.

```rust
let mut transaction = service.begin().await.context("Failed to create transaction")?;
let result = service.list_tasks(&mut transaction, ...).await?;
transaction.commit().await.context("Failed to commit transaction")?;
```

See also: [database patterns](database-patterns.md) · [testing patterns](testing-patterns.md) · [integrations](integrations.md).

_Last updated: 2026-10-15_
