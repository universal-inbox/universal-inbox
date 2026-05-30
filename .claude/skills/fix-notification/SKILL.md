---
name: fix-notification
description: Load a specific Universal Inbox notification into a fresh worktree for debugging — creates a worktree with isolated ports, restores the prod DB backup, gives the owning user a Local password auth, starts the API/workers/web, and leaves the user logged in at /notifications/{id}. USE WHEN debug notification, reproduce notification bug, load notification, fix notification, notification preview bug.
user-invocable: true
---

# `fix-notification`

End-to-end workflow to load a specific Universal Inbox notification into a
disposable local environment for debugging. Stops once the user is logged
in and the notification preview is visible — the actual fix is your job
after that.

The canonical reference for the underlying commands is `CLAUDE.md`
(sections "Worktree Development Workflow" and "Playwright Browser
Testing"). This skill orchestrates them in the right order plus the few
non-obvious bits (status flip, headless process-compose, password reset
via stdin).

## Inputs

- **`notification_id`** (required) — UUID of the notification. First arg.
  If missing, ask the user.
- **`backup_path`** (optional) — path to the `pg_dump` directory backup.
  Second arg. Default: `/tmp/prod.bak`.

Validate before doing anything else:
- `notification_id` matches `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`.
- `backup_path` exists on disk.

A useful short suffix for the worktree name: the first 8 chars of the
UUID. Example: `debug-notif-ea1b4560`.

## Step 1 — Create an isolated worktree

`wt` is normally a shell function. In non-interactive contexts call the
binary directly and pass `--yes` to skip the pre-start hook prompts.

```bash
WT_BIN="$(which -a wt | tail -1)"
"$WT_BIN" switch --create "debug-notif-<short-uuid>" --yes
```

This triggers the project's pre-start hooks: copies gitignored state,
wipes stale pg/redis data dirs, generates `.local_envrc` with
branch-hashed ports (`PGPORT`, `REDIS_PORT`, `DX_SERVE_PORT`, `API_PORT`,
`PROCESS_COMPOSE_PORT`), runs `direnv allow`, and `npm install`.

The worktree path is `~/Dev/universal-inbox/universal-inbox.<branch>`.

## Step 2 — Bootstrap env (devbox + initdb)

Every subsequent command must be wrapped in `direnv exec .` from the
worktree directory so the per-worktree ports are in scope.

```bash
cd ~/Dev/universal-inbox/universal-inbox.debug-notif-<short-uuid>
direnv exec . true
```

The first invocation runs devbox (installs cargo, playwright chromium,
etc.) and `initdb`s the per-worktree PostgreSQL data directory. Expect
this to take 30–60s the first time.

Capture the ports so later steps can use them:

```bash
API_PORT=$(direnv exec . printenv API_PORT)
DX_SERVE_PORT=$(direnv exec . printenv DX_SERVE_PORT)
PGPORT=$(direnv exec . printenv PGPORT)
PROCESS_COMPOSE_PORT=$(direnv exec . printenv PROCESS_COMPOSE_PORT)
```

## Step 3 — Start PostgreSQL + Redis headless

`just run` defaults to the TUI which fails in non-interactive sessions.
Start `process-compose` directly with `-t=false`:

```bash
nohup direnv exec . process-compose \
  -f .devbox/virtenv/redis/process-compose.yaml \
  -f process-compose-pg.yaml \
  -f process-compose.yaml \
  -p "$PROCESS_COMPOSE_PORT" -t=false \
  > /tmp/pc-debug-notif-<short-uuid>.log 2>&1 &
```

Poll until pg + redis are up (the `ui-*` services start disabled and
must be opted into in Step 8):

```bash
until direnv exec . just status 2>/dev/null \
    | grep -E "postgresql|redis" | grep -qv "Disabled"; do sleep 1; done
```

## Step 4 — Restore the prod backup

`restore-db-backup` accepts a `pg_dump` directory backup.

```bash
direnv exec . just restore-db-backup <backup_path>
direnv exec . just api ensure-db
```

`ensure-db` creates the DB if missing and applies any newer migrations
on top of the restored snapshot. Skipping it leaves the schema at the
backup's snapshot version, which may break the app if migrations have
landed since.

## Step 5 — Resolve the user owning the notification

```bash
direnv exec . psql -h /tmp -U postgres -p "$PGPORT" -d universal-inbox -tA -c \
  "SELECT u.id, u.email
   FROM notification n
   JOIN \"user\" u ON u.id = n.user_id
   WHERE n.id = '<notification_id>';"
```

Output format: `<user-uuid>|<email>`. Parse both. If empty, abort with
a clear message — the notification isn't in this backup.

## Step 6 — Auto-flip status if `Deleted`

A `Deleted` notification won't appear in the inbox UI, so the
`/notifications/<id>` route will silently redirect to whatever was
previously selected. Check the status and flip if needed:

```bash
STATUS=$(direnv exec . psql -h /tmp -U postgres -p "$PGPORT" -d universal-inbox -tA -c \
  "SELECT status FROM notification WHERE id = '<notification_id>';")

if [ "$STATUS" = "Deleted" ]; then
  direnv exec . psql -h /tmp -U postgres -p "$PGPORT" -d universal-inbox -c \
    "UPDATE notification SET status = 'Unread' WHERE id = '<notification_id>';"
  STATUS_FLIPPED=true
fi
```

Surface the flip in the hand-off (Step 9) so the user can revert before
any destructive testing.

## Step 7 — Reset the user's password

Use the `reset-password` recipe (lives at `api/.justfile:66-67`, backed
by `cargo run -- user reset-password <email>` →
`api/src/commands/user.rs::reset_password`). It reads the new password
from stdin in non-TTY mode and either updates the existing Local auth
row or creates one if the user only had OIDC/Passkey:

```bash
echo "test123456" | direnv exec . just api reset-password "<email>"
```

Use `test123456` as the password by convention — it matches the
project's seeded test users (`api/tests/browser/helpers.rs:34`) so it's
already in everyone's muscle memory.

The recipe does **not** touch `email_validated_at`. If the user's email
isn't already validated (rare for prod backup users, but possible),
their login attempt will be rejected. Validate it explicitly:

```bash
direnv exec . psql -h /tmp -U postgres -p "$PGPORT" -d universal-inbox -c \
  "UPDATE \"user\" SET email_validated_at = COALESCE(email_validated_at, now()) WHERE id = '<user-id>';"
```

## Step 8 — Start the app services and wait for readiness

```bash
direnv exec . just start ui-api
direnv exec . just start ui-workers
direnv exec . just start ui-web
```

Cold compile of `ui-api` is ~90s. Wait for it before continuing:

```bash
until curl -sf "http://localhost:$API_PORT/ping" | grep -q healthy; do sleep 3; done
until curl -sf -o /dev/null -w "%{http_code}" "http://localhost:$DX_SERVE_PORT/" | grep -q "200"; do sleep 5; done
```

## Step 9 — Hand-off

Print a single concise summary block. Do **not** launch a browser — the
user does that step. The hand-off should be self-contained so the user
can act without scrolling back through tool output.

Template:

```
Ready to debug notification <notification_id>

  Worktree:        <worktree-path>
  Login URL:       http://localhost:<DX_SERVE_PORT>/login
  Notification:    http://localhost:<DX_SERVE_PORT>/notifications/<notification_id>
  Email:           <email>
  Password:        test123456
  API:             http://localhost:<API_PORT>
  Process-compose: http://localhost:<PROCESS_COMPOSE_PORT>

  Status flip: <Yes — was Deleted, now Unread (revert before destructive testing)> | <No — was already <status>>
```

Then stop. Let the user open the login URL and continue the actual fix
work.

## Cleanup (not run by the skill)

The skill creates state on disk: a worktree with its own PG data dir, a
Local auth row, and possibly a notification status flip. None of this
is cleaned up automatically — that's the user's call.

Full teardown of the whole worktree (including the DB):

```bash
wt remove debug-notif-<short-uuid>
```

The `pre-remove.stop_services` hook will shut process-compose down
cleanly before deleting the directory.

## Troubleshooting

- **`psql … notification ea1b4560-…` returns 0 rows.** The notification
  isn't in this backup snapshot. Either the backup pre-dates it, or it
  was hard-deleted. Abort.
- **Notification page redirects to a different ID after navigation.**
  Status is probably `Deleted` and Step 6 was skipped. Re-run Step 6.
- **`just api reset-password` errors with "Unable to find user".**
  Email parsing went wrong in Step 5. Print the raw psql output and
  reparse.
- **JS changes to `web/js/index.js` don't take effect.** Unrelated to
  this skill, but a known dev-loop gotcha: `just web bundle-js` only
  refreshes `web/public/js/index.js`. The wasm-bindgen snippet at
  `web/public/snippets/<crate-hash>/public/js/index.js` is regenerated
  only on a Rust rebuild, so JS-only iterations are served stale. Force
  a fresh wasm-bindgen pass by `rm -rf target/dx/universal-inbox-web`
  and restarting `ui-web`, or `cp web/public/js/index.js` over the
  snippet path as a stopgap.
