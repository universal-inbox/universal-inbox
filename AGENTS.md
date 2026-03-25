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
- **External Services**: Redis (cache/sessions)
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

### Key Environment Variables
```bash
DATABASE_URL="postgres://postgres:password@127.0.0.1:5432/universal-inbox"
SQLX_OFFLINE="true"  # Use cached query metadata
RUST_MIN_STACK=104857600  # Required for tests (large nested structs)
```

## Worktree Development Workflow (worktrunk + direnv + devbox)

The project uses [worktrunk](https://worktrunk.dev) (`wt`) to manage git worktrees for parallel
agent workflows, [direnv](https://direnv.net) to load per-worktree environment variables, and
[devbox](https://www.jetpack.io/devbox/) to provision tools and services. Each worktree gets its
own isolated set of ports (PostgreSQL, Redis, API, web, process-compose) derived from a hash of
the branch name, so several worktrees can run side-by-side without colliding.

### Creating a worktree

```bash
# Interactive shell (recommended for humans)
wt switch --create my-feature

# Non-interactive (agents, scripts): bypass the approval prompt for pre-start hooks.
# `wt` is normally a shell function — call the binary directly when no TTY is attached.
$(which -a wt | tail -1) switch --create my-feature --yes
```

`wt switch --create` runs the pre-start hooks defined in `.config/wt.toml`:

1. `copy` — `wt step copy-ignored` clones gitignored state (e.g. `.devbox/`) into the worktree.
2. `clean_data` — wipes any stale PostgreSQL/Redis data dirs.
3. `fix_permissions` — `chmod -R og-rwx .devbox/virtenv/postgresql_17` (required by `initdb`).
4. `env` — generates `.local_envrc` with branch-hashed ports:
   - `PGPORT`, `REDIS_PORT`, `DX_SERVE_PORT`, `API_PORT`, `PROCESS_COMPOSE_PORT`
   - `DATABASE_URL`, `UNIVERSAL_INBOX__APPLICATION__*`, `UNIVERSAL_INBOX__DATABASE__PORT`,
     `UNIVERSAL_INBOX__REDIS__PORT`
   - `PGHOST=/tmp` (the default Unix socket path is too long)
5. `setup` — `direnv allow` so direnv loads the envrc on entry.
6. `web_install` — `cd web && npm install`.

The first direnv load (`direnv exec . env`, `direnv allow`, or `cd` into the worktree) also runs
`devbox` to install missing tools (cargo, rustup toolchains, playwright chromium, …) and
`initdb`s the PostgreSQL data directory.

### Running commands inside a worktree (non-interactive agents)

Agents that don't keep shell state across calls must wrap commands with `direnv exec .` and use
absolute paths so each invocation re-loads the worktree env. Example:

```bash
cd /Users/.../universal-inbox.my-feature && direnv exec . just status
```

Without `direnv exec .` the worktree's port overrides are not in scope and commands hit default
ports on the main checkout.

### Inspecting / removing worktrees

```bash
wt list                          # Show all worktrees with their URLs and ports
wt remove                        # Remove worktree; runs pre-remove hooks (stops process-compose)
wt switch -                      # Switch back to the previous worktree
```

`pre-remove.stop_services` invokes `process-compose down -p $PROCESS_COMPOSE_PORT`, so a clean
removal automatically shuts services down.

### Starting process-compose headless

`just run` defaults to the process-compose TUI, which fails in non-interactive environments with
`open /dev/tty: device not configured`. Agents should start the orchestrator directly without TUI:

```bash
cd /Users/.../universal-inbox.my-feature && \
  nohup direnv exec . process-compose \
    -f .devbox/virtenv/redis/process-compose.yaml \
    -f process-compose-pg.yaml \
    -f process-compose.yaml \
    -p "$PROCESS_COMPOSE_PORT" -t=false \
    > /tmp/pc.log 2>&1 &
```

PostgreSQL and Redis start automatically; the universal-inbox services (`ui-api`, `ui-workers`,
`ui-web`) are `disabled: true` and must be explicitly started with `just start <service>`.

### End-to-end smoke test in a worktree

```bash
# 1. Create worktree (gets isolated ports written to .local_envrc)
$(which -a wt | tail -1) switch --create smoke-test --yes

WT=/Users/.../universal-inbox.smoke-test
cd "$WT"

# 2. Load env once to trigger devbox + initdb
direnv exec . true

# 3. Start orchestrator headless (see "Starting process-compose headless")
nohup direnv exec . process-compose \
  -f .devbox/virtenv/redis/process-compose.yaml \
  -f process-compose-pg.yaml -f process-compose.yaml \
  -p "$(direnv exec . printenv PROCESS_COMPOSE_PORT)" -t=false \
  > /tmp/pc.log 2>&1 &

# 4. Apply migrations (creates the `universal-inbox` DB if missing)
direnv exec . just api ensure-db

# 5. Start app services
direnv exec . just start ui-api
direnv exec . just start ui-workers
direnv exec . just start ui-web

# 6. Wait for the API to compile + bind (~90s cold)
until curl -sf "http://localhost:$(direnv exec . printenv API_PORT)/ping" \
  | grep -q healthy; do sleep 3; done

# 7. Seed a test user and capture the email from the log
direnv exec . just api generate-user 2>&1 \
  | grep -oE 'test\+[a-f0-9-]+@test\.com'

# 8. Verify auth via the API (HTTP 200 + JWT cookie)
curl -s -c /tmp/ui_cookie.txt -X POST \
  "http://localhost:$(direnv exec . printenv API_PORT)/api/users/me" \
  -H 'content-type: application/json' \
  -d '{"email":"<email>","password":"test123456"}'
```

The same flow works through the browser at `http://localhost:$DX_SERVE_PORT/login`.

### Useful `just` recipes inside a worktree

```bash
just status                   # Pretty-prints state of all 5 process-compose services
just print-env-info           # Prints the URLs for web, API, PostgreSQL, Redis
just api ensure-db            # Creates the DB if missing + runs sqlx migrations
just api generate-user        # Seeds a fully-populated test user (prints email/password)
just start <service>          # ui-api | ui-workers | ui-web | caddy | build-tailwind | bundle-js
just stop <service>           # Stop a single service
just logs <service>           # Follow last 100 lines of a service log
```

## Playwright Browser Testing

This section documents how to test the Universal Inbox web application using Playwright MCP for browser automation.

### Prerequisites

1. **Worktree (recommended)**: Create one with `wt switch --create <name> --yes` so ports are isolated.
2. **Process-compose running**: Either `just run` (TUI mode for humans) or the headless command in the
   "Starting process-compose headless" section above.
3. **Playwright skill loaded**: Use `/playwright` or the `playwright-cli` skill.
4. **Environment ports**: Always read `$API_PORT`, `$DX_SERVE_PORT`, `$PROCESS_COMPOSE_PORT` from
   direnv — they vary per worktree. Defaults (`8000`, `8080`, `9999`) only apply on the main checkout.

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

## Design Context

### Users
Developers and knowledge workers who juggle notifications from 5-7+ sources (GitHub, Linear, Slack, Google Mail, Google Calendar, Todoist). They open Universal Inbox to triage their notifications quickly — deciding what needs action, what can wait, and what can be dismissed. The primary job-to-be-done is **regaining control over notification overload** so they can focus on deep work.

### Brand Personality
**Friendly, Clean, Effortless** — Universal Inbox should feel like a calm, well-organized space that handles complexity behind the scenes. It's approachable without being playful, polished without being flashy.

### Emotional Goals
- **Relief & control** — "Everything is handled, I'm on top of it" — reduces notification anxiety
- **Clarity & calm** — "I can see exactly what needs attention" — quiet confidence, no overwhelm

### Aesthetic Direction
- **Visual tone**: Clean, light-first (with full dark mode support), generous whitespace, subtle depth through shadows and borders rather than heavy color blocks
- **Reference**: Superhuman — speed-obsessed, keyboard-driven, minimal chrome, split-pane layout, polished transitions that feel fast
- **Anti-references**: Cluttered dashboards, heavy enterprise UIs with too many borders and icons, anything that feels slow or heavy
- **Theme**: Light mode as default (`data-theme="light"`), dark mode fully supported (`data-theme="dark"`)
- **Typography**: DM Sans as the primary UI font (`--font-ui`), with a cross-platform monospace stack for code/tokens (`--font-mono`)
- **Icons**: Lucide (primary) + MDI (integration brand icons) via Iconify

### Design Principles

1. **Triage speed above all** — Every interaction should be optimizable to a single keystroke. The UI should never make the user wait, think, or click twice for common actions. Keyboard shortcuts are first-class.

2. **Calm information density** — Show enough context to make decisions without opening each item, but never overwhelm. Use progressive disclosure (collapsed cards, detail panel, overflow menus) to layer complexity.

3. **Consistent visual language** — Every color, radius, shadow, and spacing value comes from a design token. Components share a common shape language. No one-off styles that break the system.

4. **Accessible by default** — 44px minimum touch targets, WCAG AA contrast ratios, `prefers-reduced-motion` support, focus-visible indicators, ARIA roles, and keyboard navigation built into every component.

5. **Theme-aware everything** — Every UI element must render correctly in both light and dark themes using CSS custom properties. No hardcoded colors outside token definitions.

### Design Token System
The design system is defined in `design-proposal-notifications-page.html` with the `--ui-*` prefix convention:
- **Colors**: `--ui-primary`, `--ui-success`, `--ui-error`, `--ui-warning`, `--ui-info`, `--ui-purple`, plus surface/border/muted variants
- **Brand colors**: `--brand-github`, `--brand-linear`, `--brand-slack`, `--brand-google`, `--brand-gcal`, `--brand-todoist`, `--brand-notion`
- **Typography**: `--font-ui` (DM Sans), `--font-mono`, scale from `--ui-text-2xs` (9px) to `--ui-text-2xl` (22px)
- **Radii**: `--ui-radius-xs` (3px) through `--ui-radius-pill` (9999px)
- **Shadows**: `--ui-shadow-sm`, `--ui-shadow-md`, `--ui-shadow-lg`
- **Focus**: `--ui-focus-ring` (consistent focus indicator)
- **Motion**: `--ui-ease` (standard), `--ui-ease-out` (expressive)

## Frontend Styling & Component Rules

The web frontend uses **TailwindCSS v4 + FlyonUI + Dioxus components** as the styling stack. Custom CSS in `web/css/universal-inbox.css` is the layer of last resort — only for things the other three cannot express.

**Default to TailwindCSS v4 and FlyonUI utilities for everything; fall back to a custom CSS class only when the styling is genuinely too complex to express as a utility composition** (pseudo-elements, keyframes, parent/sibling cascades, scrollbar internals, form-control internals, etc. — see the "last resort" list below).

### Decision order (apply top-down)

When you need to style something, evaluate options in this order and stop at the first that fits:

1. **Tailwind v4 utility classes** backed by the project's `@theme` tokens.
   - Colors: `bg-ui-primary`, `text-ui-error`, `border-ui-border`, `bg-sidebar-active-bg`, `text-brand-github`, `bg-avatar-hue-7`
   - Radii: `rounded-ui-xs`, `rounded-ui-sm`, `rounded-ui-md`, `rounded-ui-lg`, `rounded-ui-pill`
   - Shadows: `shadow-ui-sm`, `shadow-ui-md`, `shadow-ui-lg`
   - Fonts: `font-ui`, `font-mono`
   - Dark mode: handled automatically by the `@custom-variant dark` rule and `[data-theme="dark"]` token overrides — utilities reactively re-resolve. Never duplicate values for dark mode.
   - **Never** hardcode hex colors, pixel radii, shadow definitions, or font stacks in component code. If you find yourself writing `bg-[#388fef]`, the right answer is `bg-ui-primary`. If the token doesn't exist yet, add it to the `@theme inline` block at the top of `web/css/universal-inbox.css` rather than inlining the literal.
2. **FlyonUI native components** (`btn`, `badge`, `alert`, `modal`, `dropdown`, `toggle`, `tabs`, `accordion`, `tooltip`, `input`) when they're a faithful match. The FlyonUI theme plugin already maps the brand palette to FlyonUI's `--color-*` tokens, so its components inherit the design language for free.
3. **A Dioxus component from `web/src/components/ui/`** — see the inventory below. If a pattern is repeated 3+ times across the app, it belongs in a component, not as a duplicated class string.
4. **A new Dioxus component in `web/src/components/ui/`** if the pattern is reusable but missing. Add a doc-comment with usage examples and register it in `web/src/components/ui/mod.rs`.
5. **Custom CSS in `web/css/universal-inbox.css`** — only as a last resort, and only for one of these:
   - Pseudo-element decorations (`::before` / `::after`) that can't be expressed as utilities
   - `@keyframes` animations + the classes that bind to them
   - Complex sibling/child/parent-state selectors (`.parent.expanded > .child`, `:checked + .track + .thumb`)
   - Scrollbar styling (`::-webkit-scrollbar*`, `scrollbar-color`)
   - Form-control internals that require `appearance: none` + custom decoration
   - Responsive overrides that can't fit in `sm:` / `md:` / `lg:` Tailwind variants

### Components available in `web/src/components/ui/`

Always check this list before styling something from scratch. Each component is variant-driven and token-aware:

| Component | Variants / sizes | Replaces |
|---|---|---|
| `Button` | `Primary`, `Secondary`, `Ghost`, `Danger`, `Warning`, `Passkey`, `Connect`, `Icon`, `Seed` × `Sm` / `Md` / `Lg` | All variants emit FlyonUI `btn` + modifiers (`btn-primary`, `btn-soft`, `btn-text`, `btn-error`, etc.) — no custom CSS. Legacy: `.profile-btn`, `.detail-btn`, `.dock-btn`, `.api-key-btn`, `.task-complete-btn`, `.edit-btn` |
| `Badge` | `Primary`, `Muted`, `Count`, `Email`, `Method` × optional `BadgeTone::{Success, Warning, Error, Info, Primary, Purple}` | `.nav-badge.*`, `.count-badge`, `.profile-badge`, `.profile-email-badge`, `.auth-method-badge` |
| `Tag` | `Open`, `Review`, `Error`, `Warning`, `Info`, `Muted`, `Urgent`, `Mention`, `Success` | `.tag.{open,review,error,warning,info,muted,urgent,mention,success}` |
| `StatusLeaf` | `Connected`, `Disconnected`, `Error`, `Syncing` | `.status-leaf.*` + `.leaf-dot` |
| `Card` family — `Card`, `CardHeader`, `CardMeta`, `CardBody`, `CardRight`, `CardEmptyState` | `CardVariant::{Default, ApiKeys, Integration}` | `.preview-card`, `.api-keys-card`, `.integration-card` (with `.expanded`), `.card-header`, `.card-meta`, `.card-name`, `.card-desc`, `.card-right`, `.card-body-expandable`, `.api-keys-empty-state` |
| `MetadataGrid` + `MetadataItem` | — | `.preview-meta-grid` + `.preview-meta-label` + `.preview-meta-value` pairs |
| `Kbd` + `KeyboardHint` | `KbdSize::{Xs, Sm}` | `.kbd`, `.kbd-xs`, `.ui-kbd`, `.hint`, `.keyboard-hints` |
| `BrandTile` | `BrandTileSize::{Sm, Md, Lg}`, `provider: IntegrationProviderKind` | `.brand-tile` + `.brand-glyph` |
| `NavItem` + `NavSection` | — | `.nav-item` + `.nav-icon` + `.nav-section` + `.nav-section-label` |
| `ToggleSwitch` | `ToggleSize::{Sm, Md}` | `.toggle-switch` + `.toggle-track` + `.toggle-thumb`, `.ui-toggle` |
| `EmptyState` | optional `animated_check: bool` | `.page-empty*`, `.inbox-zero-check` |
| `PageHeader` + `Overline` | — | `.page-header` + `.page-subtitle`, `.settings-section-overline` |

If you need a variant or size that doesn't exist, **extend the existing component** (add an enum variant with a class mapping). Don't fork a parallel component.

### The "CSS class hook" hybrid pattern

Some components — `Tag`, `StatusLeaf`, `Card` (Integration variant), `EmptyState` (animated_check), the notification row (`.ui-nrow*`) — emit *legacy CSS class names* alongside their utility composition. This is **deliberate**: pseudo-element decorations (`.tag.review::before` colored dot), keyframe animations (`card-body-in`, `notif-enter`, `notif-dismiss-*`, `inbox-zero-pop`), and `:checked +` sibling cascades are anchored to those class names. Removing the class hook breaks the visual.

When extending or building components like this:
- Document the load-bearing class names in the module-level doc-comment.
- Don't replace the class string with utility-only styling — the decoration would silently disappear.
- The class string is the contract between the Dioxus component and the CSS layer. Keep it intact.

### What NOT to do

- ❌ **Don't introduce new `.foo-bar` custom CSS classes** unless one of the "last resort" criteria above applies. Adding a class for spacing, color, or a one-off layout is almost always wrong; reach for utilities or a component.
- ❌ **Don't write `class: "bg-[var(--ui-primary)]"` (arbitrary value syntax)** for tokens that have a `@theme` mapping. Use `bg-ui-primary`.
- ❌ **Don't hardcode hex colors, font stacks, pixel radii, or shadows** in Rust strings. They belong in the token layer.
- ❌ **Don't duplicate dark-mode rules** — utilities and tokens already react to `[data-theme="dark"]`.
- ❌ **Don't use `style="..."` inline overrides** except when a value is genuinely dynamic (e.g., a CSS custom property `style="--source-accent: {color}"` set per-row). Static values go through utilities or tokens.
- ❌ **Don't add new modules to `web/src/components/ui/mod.rs` without a corresponding `pub use`** — components are flat-imported via `use crate::components::ui::*;` and depend on the re-export.

### When in doubt: utilities + components first, custom CSS last

The migration that produced these rules collapsed ~437 custom CSS classes into ~10 reusable Dioxus components consuming `@theme` tokens. The intent is that **adding a feature should rarely require touching `web/css/universal-inbox.css`**. If you find yourself opening that file to add styles, pause and ask whether a token, utility, or component would handle it instead.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
