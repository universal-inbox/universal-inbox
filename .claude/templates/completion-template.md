# Task Completion: <short title>

- **Issue**: <bd id>   **Date**: <YYYY-MM-DD>   **Branch/worktree**: <name>

## Summary

<2–3 sentences: what was delivered and why.>

## Files Changed

- `path/to/file.rs` — <what changed>

## Approach & Decisions

- <key decisions, trade-offs, alternatives rejected>

## Quality Gates

- [ ] `just check` (lint + compile)
- [ ] `just test` (root)  /  `cd api && just test`  /  `cd web && just test`
- [ ] Migrations run (`just api ensure-db`) if schema changed

## Verification

<how the change was confirmed: test output, curl, Playwright screenshot, etc.>

## Follow-ups

- <bd issues filed for remaining work>

## Pushed

- [ ] `git pull --rebase && bd dolt push && git push` — up to date with origin

_Last updated: 2026-05-30_
