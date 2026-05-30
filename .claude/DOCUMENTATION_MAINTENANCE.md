# Documentation Maintenance

How to keep this `.claude/` doc system healthy. Read this before adding, updating, or archiving any doc.

> **Auto-load boundary:** `CLAUDE.md`, `.claude/COMMON_MISTAKES.md`, `.claude/QUICK_START.md`, and `.claude/ARCHITECTURE_MAP.md` load every session (~1,500 tokens). Everything under `.claude/completions/**`, `.claude/sessions/**`, and `.claude/docs/archive/**` is git/claude-ignored and **never auto-loaded** — opened only on explicit request.

## 1. Update `.claude/COMMON_MISTAKES.md`

Auto-loaded and CRITICAL — every token counts. Keep it to the **top 5** mistakes only.

```
Did an error cause a critical bug, production incident, or repeat agent failure?
├─ No  → don't touch COMMON_MISTAKES.md. Record it in a learnings doc instead.
└─ Yes → Is it already in the top 5?
         ├─ Yes → sharpen the wording / add the fix.
         └─ No  → add it. Already 5 entries?
                  └─ Demote the least-recently-hit entry to
                     [.claude/docs/learnings/common-pitfalls.md](docs/learnings/common-pitfalls.md).
```

Example: agents kept forgetting `RUST_MIN_STACK=104857600`, causing stack overflows in tests → promote to COMMON_MISTAKES.md, demote the stalest entry to common-pitfalls.

## 2. Create a completion doc

At the end of **every non-trivial task**, copy [.claude/templates/completion-template.md](templates/completion-template.md) into `.claude/completions/` (e.g. `2026-05-30-slack-rendering-fix.md`). Skip only for one-line/throwaway edits. Never auto-loaded — written for the next agent who picks up similar work.

## 3. Create a session doc

For **multi-session work** (a feature spanning days), copy [.claude/templates/session-template.md](templates/session-template.md) into `.claude/sessions/active/`. When the work ships, move it to `.claude/sessions/archive/`.

```
One sitting?        → completion doc (§2), no session doc.
Spans sessions?     → session doc in sessions/active/ → move to sessions/archive/ when done.
```

## 4. Archive superseded docs

Planning docs, POC summaries, and guides replaced by newer ones move to [.claude/docs/archive/](docs/archive/README.md). Don't delete — archive keeps the rationale trail. Never auto-loaded.

Example: a migration plan that's now fully implemented → `.claude/docs/archive/`.

## 5. Update learnings

When a **new reusable pattern or best practice** emerges, add it to the matching file under `.claude/docs/learnings/`:

| Topic | File |
|---|---|
| Tests, fixtures, `TestedApp`, mocks | [testing-patterns.md](docs/learnings/testing-patterns.md) |
| Migrations, SQLx, repository, transactions | [database-patterns.md](docs/learnings/database-patterns.md) |
| Routes, errors, services | [api-design.md](docs/learnings/api-design.md) |
| Dioxus, components, styling | [frontend-dioxus.md](docs/learnings/frontend-dioxus.md) |
| GitHub/Linear/Slack/Todoist/Google clients | [integrations.md](docs/learnings/integrations.md) |
| One-off gotchas, demoted mistakes | [common-pitfalls.md](docs/learnings/common-pitfalls.md) |

Each learnings doc stays ~500-800 tokens. Loaded only when the task touches that area. Register new ones in [.claude/LEARNINGS_INDEX.md](LEARNINGS_INDEX.md) and [.claude/docs/INDEX.md](docs/INDEX.md).

## 6. Keep `CLAUDE.md` under 200 lines

It's a slim hub. Push detail **down** into the docs above and link — never duplicate. If a section grows past a paragraph, move it to a learnings doc and leave a one-line pointer. For deep specifics, link to [the comprehensive guide](../AGENTS.md).

_Last updated: 2026-05-30_
