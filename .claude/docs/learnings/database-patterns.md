# Database Patterns

SQLx 0.8 + PostgreSQL.

## Repository trait pattern

Each domain has an `#[async_trait]` repository trait. Methods take a borrowed transaction
as the executor — they never own a pool or open their own transaction:

```rust
#[async_trait]
pub trait NotificationRepository {
    async fn get_one_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: NotificationId,
    ) -> Result<Option<Notification>, UniversalInboxError>;

    async fn create_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError>;
}
```

The `&mut Transaction<'_, Postgres>` executor lets a service compose several repository
calls into one atomic unit, and lets tests roll everything back.

## Transaction lifecycle

Services expose `begin()`; the caller opens, does the work, then commits. Add `.context(...)`
on every step so error chains stay readable:

```rust
let mut transaction = service
    .begin()
    .await
    .context("Failed to create transaction")?;

let result = service
    .list_tasks(&mut transaction, ...)
    .await?;

transaction
    .commit()
    .await
    .context("Failed to commit transaction")?;
```

Dropping the transaction without `commit()` rolls back — the default on any early `?` return.

`Repository::begin` (the services' `begin()` delegates to it) also sets `app.audit_actor` on the transaction so the audit triggers can record who changed a notification or task. Pool exhaustion surfaces as `DatabaseUnavailable` (503), not `DatabaseError`.

## Status types

Mutations return explicit status so callers know what changed:

```rust
// updates
pub struct UpdateStatus<T> {
    pub updated: bool,
    pub result: Option<T>,
}

// upserts
pub enum UpsertStatus<T: Clone> {
    Created(T),
    Updated { old: T, new: T },
    Untouched(T),
}
```

`UpdateStatus` drives 200-vs-not-modified responses; `UpsertStatus` lets sync jobs detect
real changes (e.g. only re-notify when `Created` / `Updated`).

## Sensitive data at rest

Personal data and secrets are not stored in plaintext. `api/src/repository/data_encryption.rs` lists every encrypted column in `ENCRYPTED_COLUMNS` (`*_enc` columns, sealed with AAD tied to the row id; OAuth tokens too). Lookups by email use blind indexes. Add a new sensitive column there so the `data-encryption` CLI (encrypt-plaintext, key rotation, status) covers it. In the domain crate wrap such values in `Pii<T>` (`Pii<String>` for third-party payloads); never log or span-record them. `just check-pii-logging` lints `expose()` inside tracing macros. The legacy plaintext columns were dropped by migration `20261015090000_drop_plaintext_columns`, which refuses to run while any is non-NULL, so run `data-encryption encrypt-plaintext` first.

## Cached values

`TracedRedisCache` (`api/src/utils/cache.rs`) stores values as JSON, not the `cached` crate's default msgpack, which mis-decodes tagged serde enums (e.g. Slack blocks). Keep new cached types JSON-safe.

## Migrations

Live in `api/migrations/`, named `YYYYMMDDHHMMSS_description.{up,down}.sql` — always write
both the `.up.sql` and the matching `.down.sql`.

```bash
just api migrate-db     # apply pending migrations
just api ensure-db      # create the universal-inbox DB if missing, then migrate
cd api && just check-db # runs `cargo sqlx prepare` for the API bin, then `cargo check --tests`
```

## SQLX_OFFLINE + cached query metadata

CI and wasm/release builds run with `SQLX_OFFLINE="true"`, so compile-time-checked queries
(`sqlx::query!`) resolve against cached metadata instead of hitting Postgres. After changing
any `query!` / `query_as!` SQL or the schema, regenerate the cache:

```bash
cd api && just check-db        # refresh .sqlx/ metadata, then commit it
```

`just check-db` does this with `MBX_DISABLE=1`, since mbx can delete `.sqlx` files. A newer local `sqlx-cli` adds `origin` fields to every `.sqlx` file: revert unrelated diffs and strip `origin` from new files. Forgetting this makes offline builds fail with "no cached data for this query". Commit the
updated `.sqlx/` files alongside the migration and the code change.

_Last updated: 2026-10-15_
