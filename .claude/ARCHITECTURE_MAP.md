# Architecture Map

> Auto-loaded. Compact workspace map + "where to find X". For deep dives see [learnings](docs/learnings/) and the [comprehensive guide](../AGENTS.md).

## Workspace tree (cargo workspace, all Rust, edition 2024)

```
universal-inbox/
├── src/                      # SHARED domain crate (root) — pure types, no I/O
│   ├── auth/ integration_connection/ notification/ slack_bridge/
│   ├── task/ third_party/ user.rs typed_id.rs utils/ lib.rs
├── api/                      # Backend — Actix-web 4, SQLx 0.8 (PG), Tokio, Apalis
│   ├── src/
│   │   ├── routes/           # HTTP handlers (auth, notification, task, oauth, webhook, ...)
│   │   ├── universal_inbox/  # Business-logic services (the core)
│   │   ├── repository/       # DB layer (SQLx, &mut Transaction)
│   │   ├── integrations/     # Third-party clients (github/ linear/ slack todoist google_* ...)
│   │   ├── jobs/             # Apalis background job handlers
│   │   ├── mcp/ middlewares/ mailer/ commands/
│   │   ├── configuration.rs observability.rs main.rs   # ← API entry point
│   ├── migrations/           # SQLx: YYYYMMDDHHMMSS_*.{up,down}.sql
│   ├── tests/                # Integration tests; fixtures/ = JSON
│   └── config/               # dev.toml prod.toml local.toml (gitignored)
└── web/                      # Frontend — Dioxus 0.6 → wasm32
    └── src/
        ├── pages/            # Page-level components (routes)
        ├── components/       # incl components/ui/ design-system
        ├── services/         # API client services
        ├── layouts/ model.rs route.rs theme.rs keyboard_manager.rs main.rs  # ← web entry
```

## Where to find X

| Want to touch… | Go to |
|---|---|
| HTTP endpoint / route handler | `api/src/routes/` |
| Business logic / services | `api/src/universal_inbox/` |
| Database queries / persistence | `api/src/repository/` |
| Third-party API clients | `api/src/integrations/` |
| Background jobs (Apalis) | `api/src/jobs/` |
| Shared domain types | `src/` (root crate) |
| UI pages | `web/src/pages/` |
| Frontend API clients | `web/src/services/` |
| Design-system components | `web/src/components/ui/` |
| DB migrations | `api/migrations/` |
| Integration tests + fixtures | `api/tests/` |
| Config | `api/config/*.toml` |

## Key entry points

- `api/src/main.rs` — backend server bootstrap
- `web/src/main.rs` — Dioxus WASM app
- `src/lib.rs` — shared domain crate root

## Deep dives

[testing](docs/learnings/testing-patterns.md) · [database](docs/learnings/database-patterns.md) · [api](docs/learnings/api-design.md) · [frontend](docs/learnings/frontend-dioxus.md) · [integrations](docs/learnings/integrations.md) · [pitfalls](docs/learnings/common-pitfalls.md)

_Last updated: 2026-05-30_
