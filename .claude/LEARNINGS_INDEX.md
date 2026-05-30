# Learnings Index

Task-specific deep-dive docs. **Load only the one(s) you need** — each is scoped to a domain.
For full navigation, see [docs/INDEX.md](docs/INDEX.md).

## Available learnings

| Doc | Covers | ~Tokens |
|-----|--------|---------|
| [testing-patterns](docs/learnings/testing-patterns.md) | rstest fixtures, `TestedApp`, mock servers, `RUST_MIN_STACK` | ~600 |
| [database-patterns](docs/learnings/database-patterns.md) | SQLx migrations, repository traits, transactions, `Update`/`UpsertStatus` | ~700 |
| [api-design](docs/learnings/api-design.md) | Actix routes, `UniversalInboxError` → HTTP, service/`Arc`/`Weak` patterns | ~700 |
| [frontend-dioxus](docs/learnings/frontend-dioxus.md) | Dioxus 0.6 WASM, components/ui, Tailwind v4 + FlyonUI, theme tokens | ~800 |
| [integrations](docs/learnings/integrations.md) | Adding a source, `ThirdParty*SourceService`, OAuth2, third_party items | ~700 |
| [common-pitfalls](docs/learnings/common-pitfalls.md) | Sharp edges: SQLX_OFFLINE, worktree ports, headless process-compose | ~500 |

## Load this when…

| Task type | Load |
|-----------|------|
| Writing or fixing tests | testing-patterns |
| Migrations, repository, queries | database-patterns |
| New/changed route, error mapping, service wiring | api-design |
| UI components, styling, pages | frontend-dioxus |
| Adding/editing a GitHub/Linear/Slack/Google/Todoist source | integrations + database-patterns + api-design |
| Debugging a build/test/runtime surprise | common-pitfalls |

For deep reference beyond these, use [AGENTS.md](../AGENTS.md).

_Last updated: 2026-05-30_
