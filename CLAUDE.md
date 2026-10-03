# Universal Inbox — Claude Code Guide

> Slim navigation hub. Detail lives in the `.claude/` docs. Load only what the task needs. Keep this file under 200 lines —
> link to the dedicated docs, don't duplicate them.

Universal Inbox centralizes notifications and tasks from many sources (GitHub, Linear, Slack,
Todoist, Google Mail/Calendar/Drive) into one unified inbox so knowledge workers triage fast and
regain focus. All-Rust cargo workspace: shared domain crate (`src/`), Actix-web API (`api/`), and
a Dioxus WASM frontend (`web/`).

## Session Start Protocol

At the start of each session, read these 3 files:

- `.claude/COMMON_MISTAKES.md` — the six failures that recur on this stack
- `.claude/QUICK_START.md` — build/test/DB/service commands + required env
- `.claude/ARCHITECTURE_MAP.md` — directory map + "where to find X"

**Then load task-specific docs (~500–800 each) — see [`.claude/docs/INDEX.md`](.claude/docs/INDEX.md):**

- API / backend work → [`api-design.md`](.claude/docs/learnings/api-design.md) + [`database-patterns.md`](.claude/docs/learnings/database-patterns.md)
- Frontend work / styling → [`frontend-dioxus.md`](.claude/docs/learnings/frontend-dioxus.md)
- Adding an integration → [`integrations.md`](.claude/docs/learnings/integrations.md) + `database-patterns.md` + `testing-patterns.md`
- Writing tests → [`testing-patterns.md`](.claude/docs/learnings/testing-patterns.md)
- Tracing / span attributes → [`observability.md`](.claude/docs/learnings/observability.md)
- Stuck / error → [`common-pitfalls.md`](.claude/docs/learnings/common-pitfalls.md) + `COMMON_MISTAKES.md`
- Fast lookups / code patterns / style → [`QUICK_REFERENCE.md`](.claude/docs/QUICK_REFERENCE.md)

Read `.claude/completions/**`, `.claude/sessions/**`, and `.claude/docs/archive/**` only when
asked: they are historical records, not current guidance.

## Documentation Navigation

| File | Purpose |
|------|---------|
| [`.claude/COMMON_MISTAKES.md`](.claude/COMMON_MISTAKES.md) | ⚠️ Top-6 critical mistakes (auto-load) |
| [`.claude/QUICK_START.md`](.claude/QUICK_START.md) | Commands: build/test, DB, services, env (auto-load) |
| [`.claude/ARCHITECTURE_MAP.md`](.claude/ARCHITECTURE_MAP.md) | Directory map + file locations (auto-load) |
| [`.claude/LEARNINGS_INDEX.md`](.claude/LEARNINGS_INDEX.md) | Pointers to topic docs |
| [`.claude/docs/INDEX.md`](.claude/docs/INDEX.md) | Master navigation + token estimates |
| [`.claude/docs/QUICK_REFERENCE.md`](.claude/docs/QUICK_REFERENCE.md) | Fast lookups, code patterns, code style |
| [`.claude/docs/learnings/`](.claude/docs/learnings) | testing · database · api-design · frontend-dioxus · integrations · common-pitfalls |
| [`.claude/DOCUMENTATION_MAINTENANCE.md`](.claude/DOCUMENTATION_MAINTENANCE.md) | When to update/archive docs |

## Issue Tracking & Session Close

Use **beads** (`bd ready` / `bd show <id>` / `bd update <id> --claim` / `bd close <id>`) for all
task tracking — never TodoWrite or markdown TODOs. `bd remember` for persistent knowledge.
Run `bd prime` for the full workflow.

**Before ending a session:** file follow-up issues, run quality gates (`just check`, `just test`),
then `bd dolt push`.

_Last updated: 2026-05-30_
