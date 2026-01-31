# Universal Inbox - Agent Guidelines

## Quick Reference

```bash
# Build & Test (prefer just over cargo, run from appropriate directory)
just test                     # Run all tests for current project
just test "pattern"           # Run specific test pattern
just test "pattern" debug     # Run tests with debug output (preferred for debugging)
just check                    # Lint and compile checks
just build                    # Build project
just format                   # Format code

# Component-specific (cd to directory first)
cd api && just test           # API backend tests
cd web && just test           # Web frontend tests

# Database
just run                      # Start PostgreSQL and Redis
just api migrate-db           # Run migrations
just api clear-cache          # Clear Redis cache

# Development servers
just api run                  # API server only
just web run                  # Web frontend only
```

## Project Architecture

### Workspace Structure
```
universal-inbox/
├── src/              # Shared domain models (root crate)
│   ├── notification/ # Notification domain types
│   ├── task/         # Task domain types
│   ├── user/         # User domain types
│   ├── integration_connection/  # Integration provider types
│   └── third_party/  # Third-party item abstractions
├── api/              # Backend server (Actix-web)
│   ├── src/
│   │   ├── routes/           # HTTP endpoint handlers
│   │   ├── repository/       # Database layer (SQLx)
│   │   ├── universal_inbox/  # Business logic services
│   │   ├── integrations/     # Third-party service clients
│   │   └── jobs/             # Background job handlers
│   ├── migrations/           # SQLx database migrations
│   └── tests/                # Integration tests
└── web/              # Frontend (Dioxus WASM)
    └── src/
        ├── services/     # API client services
        ├── components/   # Reusable UI components
        └── pages/        # Page-level components
```

### Technology Stack
- **Backend**: Actix-web 4.0, SQLx 0.8 (PostgreSQL), Tokio, Apalis (job queue)
- **Frontend**: Dioxus 0.6 (WASM), compiled to wasm32-unknown-unknown
- **Auth**: OpenID Connect, JWT (EdDSA), Passkeys/WebAuthn, Argon2 password hashing
- **External Services**: Nango (OAuth proxy), Redis (cache/sessions)
- **Integrations**: GitHub, Linear, Todoist, Slack, Google Mail/Calendar/Drive

## Code Conventions

### Import Organization
Always use this three-section pattern with blank lines between groups:
```rust
// 1. Standard library
use std::{collections::HashMap, sync::Arc};

// 2. External crates (alphabetical)
use anyhow::{anyhow, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// 3. Internal modules
use crate::{
    repository::Repository,
    universal_inbox::UniversalInboxError,
};
use universal_inbox::notification::Notification;
```

### Naming Conventions
- **snake_case**: functions, variables, modules (`list_tasks`, `user_id`, `notification`)
- **PascalCase**: types, traits, enums (`Task`, `NotificationService`, `TaskStatus`)
- **SCREAMING_SNAKE_CASE**: constants (`DEFAULT_PAGE_SIZE`)

### Error Handling
Use the `UniversalInboxError` enum with `anyhow::Context` for error chains:
```rust
pub async fn get_task(
    &self,
    executor: &mut Transaction<'_, Postgres>,
    id: TaskId,
) -> Result<Option<Task>, UniversalInboxError> {
    self.repository
        .get_one_task(executor, id)
        .await
        .context("Failed to fetch task from repository")
}
```

**Error variants** (mapped to HTTP status codes):
- `InvalidInputData` / `InvalidParameters` → 400 Bad Request
- `ItemNotFound` → 400 Bad Request
- `AlreadyExists` → 409 Conflict
- `Unauthorized` → 401 Unauthorized
- `Forbidden` → 403 Forbidden
- `DatabaseError` / `Unexpected` / `Recoverable` → 500 Internal Server Error

### Async & Service Patterns
- All service methods are async
- Use `Arc<Service>` for shared ownership
- Use `Weak<RwLock<Service>>` to prevent circular dependencies
- Use `RwLock<Service>` for interior mutability

```rust
pub struct NotificationService {
    pub(super) repository: Arc<Repository>,
    pub github_service: Arc<GithubService>,
    pub(super) task_service: Weak<RwLock<TaskService>>,  // Weak for circular ref
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
}
```

### Transaction Management
```rust
pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
    self.repository.begin().await
}

// Usage in routes:
let mut transaction = service.begin().await.context("Failed to create transaction")?;
let result = service.list_tasks(&mut transaction, ...).await?;
transaction.commit().await.context("Failed to commit transaction")?;
```

### Serialization
```rust
// Basic derives
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Task { ... }

// Tagged enums (discriminated unions)
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum FrontAuthenticationConfig {
    OIDCAuthorizationCodePKCEFlow(FlowConfig),
    Local,
    Passkey,
}

// With content field
#[serde(tag = "type", content = "content")]
pub enum SlackStarState { ... }

// Transparent wrappers
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(pub String);
```

### Validation
Use the `validator` crate:
```rust
use validator::Validate;

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

### Tracing
Add instrumentation to all public service methods:
```rust
#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(user_id = %user_id, task_id = %task_id)
)]
pub async fn update_task(...) -> Result<UpdateStatus<Task>, UniversalInboxError> {
    // ...
}
```

## Database Patterns

### Migrations
- Located in `api/migrations/`
- Naming: `YYYYMMDDHHMMSS_description.{up,down}.sql`
- Run with: `just migrate-db` or `cd api && just migrate-db`
- Check with: `cd api && just check-db`

### Repository Pattern
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

### Common Status Types
```rust
// For updates
pub struct UpdateStatus<T> {
    pub updated: bool,
    pub result: Option<T>,
}

// For upserts
pub enum UpsertStatus<T: Clone> {
    Created(T),
    Updated { old: T, new: T },
    Untouched(T),
}
```

## Testing Patterns

### Test Structure
```rust
use rstest::*;

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

### Test Helpers
- `TestedApp` fixture provides full app instance with test database
- Fixtures in `api/tests/fixtures/` (JSON test data)
- Mock servers for all third-party integrations
- Use `pretty_assertions` for readable diffs

### Running Tests
```bash
# Always prefer running from the project where files were updated
just test                      # Root project
cd api && just test            # API project
cd web && just test            # Web project

# For debugging a specific test
just test "test_name" debug
```

## Adding New Integrations

1. Create service in `api/src/integrations/new_service.rs`
2. Implement `ThirdPartyItemSourceService` or `ThirdPartyNotificationSourceService` trait
3. Add repository methods in `api/src/repository/` if needed
4. Add routes in `api/src/routes/`
5. Update `NotificationService` or `TaskService`
6. Create migrations in `api/migrations/`
7. Add tests in `api/tests/api/`

## Configuration

### Environment Files
- Development: `api/config/dev.toml`
- Production: `api/config/prod.toml`
- Local overrides: `api/config/local.toml` (gitignored)
- Environment variables override any config value

### Required Services
- PostgreSQL (primary database)
- Redis (sessions, cache, job queue)
- Nango (OAuth proxy, Docker container at http://localhost:3003)

### Key Environment Variables
```bash
DATABASE_URL="postgres://postgres:password@127.0.0.1:5432/universal-inbox"
SQLX_OFFLINE="true"  # Use cached query metadata
RUST_MIN_STACK=104857600  # Required for tests (large nested structs)
```

## Playwright Browser Testing

This section documents how to test the Universal Inbox web application using Playwright MCP for browser automation.

### Prerequisites

1. **Process-compose running**: Start with `just run` (starts PostgreSQL, Redis, and other background services)
2. **Playwright skill loaded**: Use `/playwright` or load the `playwright` skill
3. **Environment ports**: Check `$API_PORT` and `$DX_SERVE_PORT` for actual port numbers

### Step 1: Generate Test User

```bash
# Generate a test user with sample data (notifications, tasks, integrations)
just api generate-user
```

**Output format:**
```
Test user test+{uuid}@test.com successfully generated with password test123456
```

**Credentials:**
- **Email**: `test+{uuid}@test.com` (UUID is randomly generated)
- **Password**: `test123456` (hardcoded default)

The generated user includes sample data for all integrations: Todoist, GitHub, Linear, Slack, Google Mail/Calendar/Drive.

### Step 2: Start Development Servers

The following commands will start the required server via process-compose.
```bash
# Start nango (OAuth proxy)
just start nango-db
just start nango-server

# Start Universal Inbox API, workers and Web
just start ui-api
just start ui-workers
just start ui-web
```

**Wait for services to be ready:**
```bash
# Check API health
curl -s http://localhost:${API_PORT:-8000}/ping
# Expected: {"cache":"healthy","database":"healthy"}

# Check Web frontend
curl -s -o /dev/null -w "%{http_code}" http://localhost:${DX_SERVE_PORT:-8080}/
# Expected: 200
```

### Step 3: API Proxy Configuration (Automatic)

The web frontend proxies API requests. The `just web run` command **automatically updates** `web/Dioxus.toml` with the correct API port from `$API_PORT` environment variable.

**Note**: The `dx` CLI does not support passing proxy URL as a command-line argument, so the justfile task uses `sed` to update the config before starting the server.

### Step 4: Login Test with Playwright MCP

Execute these Playwright MCP tool calls in sequence:

#### 4.1 Navigate to Login Page
```
skill_mcp(mcp_name="playwright", tool_name="browser_navigate", 
          arguments={"url": "http://localhost:${DX_SERVE_PORT}/login"})
```

#### 4.2 Wait for Page Load and Get Element Refs
```
skill_mcp(mcp_name="playwright", tool_name="browser_wait_for", 
          arguments={"time": 5})
skill_mcp(mcp_name="playwright", tool_name="browser_snapshot", arguments={})
```

**Expected snapshot elements:**
- `textbox "Email*"` - Email input field
- `textbox "Password*"` - Password input field  
- `button "Log in"` - Submit button

#### 4.3 Fill Login Form
```
# Fill email (use ref from snapshot, typically e14)
skill_mcp(mcp_name="playwright", tool_name="browser_type",
          arguments={"ref": "<email_ref>", "text": "<email_from_step_1>", "element": "Email input"})

# Fill password (use ref from snapshot, typically e18)
skill_mcp(mcp_name="playwright", tool_name="browser_type",
          arguments={"ref": "<password_ref>", "text": "test123456", "element": "Password input"})
```

#### 4.4 Submit Form and Verify
```
# Click login button (use ref from snapshot, typically e22)
skill_mcp(mcp_name="playwright", tool_name="browser_click",
          arguments={"ref": "<submit_ref>", "element": "Log in button"})
```

**Expected result:**
- Page URL changes to `/` (notifications page)
- Snapshot shows notification list with items like:
  - "Inbox" link with notification count
  - Table rows with notifications from various integrations

#### 4.5 Take Screenshot (Evidence)
```
skill_mcp(mcp_name="playwright", tool_name="browser_take_screenshot",
          arguments={"type": "png", "filename": "login-success.png"})
```

### Success Criteria

The login test is successful when:
- [ ] Page redirects from `/login` to `/` (root/notifications page)
- [ ] Snapshot shows notification table (not login form)
- [ ] Notification count > 0 in sidebar (test user has 9+ sample notifications)
- [ ] No error messages visible on page

### Troubleshooting

**Page shows "Loading..." indefinitely:**
- Check if API is responding: `curl http://localhost:${API_PORT}/ping`
- Verify `web/Dioxus.toml` proxy backend URL matches API port

**Login fails with "Invalid email address or password":**
- Regenerate test user: `just api generate-user`
- Verify password is exactly `test123456`

**Empty snapshot:**
- Wait longer for WASM to load: `browser_wait_for(time=10)`
- Check browser console for errors in snapshot output

## Documentation Resources

- **Rust std lib**: https://doc.rust-lang.org/stable/std/
- **Crate docs**: https://docs.rs/
- **Universal Inbox docs**: https://doc.universal-inbox.com
