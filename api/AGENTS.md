# API Crate — Agent Guidelines

## Project Structure

- `src/routes/` — HTTP handlers (thin wrappers)
- `src/universal_inbox/*/service.rs` — Business logic (services)
- `src/integrations/` — Third-party API clients (Linear, Slack, GitHub, etc.)
- `src/repository/` — Database access layer
- `src/configuration.rs` — App configuration
- `src/utils/` — Shared utilities (crypto, cache, JWT)

## Route Handler Rules

Route handlers in `src/routes/` **must be thin wrappers**. They should ONLY:

1. Decode/extract HTTP parameters (path params, query params, request body)
2. Parse authentication claims (e.g., extracting `user_id` from JWT)
3. Call service layer methods
4. Serialize the result into an HTTP response (status code, headers, body)

**All business logic belongs in the service layer** (`src/universal_inbox/*/service.rs`), including:

- Data validation beyond HTTP-level parsing
- External API calls (OAuth token exchange, etc.)
- Encryption/decryption operations
- State management (Redis, etc.)
- Domain-specific logic

## Transaction Management

Database transactions are **created and committed in route handlers**, not in services:

- Route handlers call `service.begin()` to start a transaction
- The transaction is passed as `executor: &mut Transaction<'_, Postgres>` to service methods
- Route handlers call `transaction.commit()` after the service method succeeds
- On error, the transaction is automatically rolled back when dropped

This keeps transaction boundaries visible at the HTTP layer and allows multiple service calls to share a single transaction.

This separation ensures:
- **Testability** — services can be tested without HTTP
- **Reusability** — services can be called from background workers/jobs
- **Clarity** — route handlers are easy to review for correctness

## Build & Test Commands

```bash
just build               # Build
just check               # Check (faster than full build)
just test                # Run tests
just check-format        # Check formatting
just format              # Auto-format
just check-commit        # Full pre-commit check
```
