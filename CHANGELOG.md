# Universal Inbox Changelog

## Unreleased

### Added

- Custom Slack reaction emoji picker in integration settings
- Add Slack browser-extension bridge for 2-way sync (delete/unsubscribe actions)
- Support multiple authentication methods per user account
- Add MCP (Model Context Protocol) server for AI agent integration
- Add configurable emoji on Slack task completion
- Hide quoted content in Gmail email previews
- Internalize OAuth for GitHub, Slack, Todoist, Google Mail, Google Calendar, and Google Drive (replace Nango).
- Add TickTick integration with internalized OAuth2 (PKCE), task sync, plan/link flows, and emoji-aware task completion
- Support hosted MCP clients (Claude, ChatGPT, Gemini, Mistral) via Client ID Metadata Document (CIMD) discovery and an allow-listed Dynamic Client Registration flow (MCP 2025-11-25 auth spec)
- Add per-task scheduled time, duration, and timezone configuration in integration settings and the planning modal
- Sync per-task time, duration, and timezone to TickTick (duration modeled as a start/due time range)
- Self-service account deletion: `DELETE /api/users/me` (confirmed by re-typing the email address) and a "Delete my account" card on the Profile page; the `user delete` CLI command runs the same service flow
- Pause the OAuth connections (every provider) of users inactive for 90 days and the connections failing for 30 days (`pause-integration-connections` cron, disabled by default): their provider grant is revoked and they show as Paused with a reconnect action; inactive users are warned by email 7 days before, and users are emailed with a reconnect link once paused, one email listing all their affected connections
- `integration-connection pause-without-email` command: before enabling the `pause-integration-connections` cron, pause without any email the connections of users inactive since a given date and the already long failing ones, so the cron only emails about users and connections that become inactive or start failing afterwards
- Change the password from Profile > Authentication methods
- Show attendee replies (answer and comment) to Google Calendar invitations received in Gmail
- Show every comment and reply of a GitHub discussion in its preview, collapsing the already read ones
- Show a check mark on GitHub discussion notifications that have an accepted answer
- Export all your data as a JSON file from the Profile page (`GET /users/me/export`, 2 exports per minute)

### Changed

- Full redesign of the user interface
- Switch Slack preview rendering to direct HTML via `slack-blocks-render` v0.5.0, replacing the Markdown→comrak→regex pipeline
- Load the Crisp support chat only when the user clicks "Support": no Crisp request, websocket or cookie before that, and none on the login/signup pages
- Mark Slack connections as Failing, with a reconnect message, as soon as the user removes Universal Inbox from Slack or an admin uninstalls the app
- Collapse the already read part of a Slack thread in its preview and open it scrolled to the latest read reply
- Configure the OTLP trace and log export levels independently (`otel_trace_directive`, `otel_log_directive`), stop tracing `/ping`, and flush pending telemetry on shutdown
- Serve the web application precompressed (brotli / gzip), with long-lived caching of content-hashed assets
- Make the Redis per-command response timeout configurable (`redis.response_timeout_in_milliseconds`)
- Make email (SMTP) settings optional: without an `application.email` section the server starts with emails disabled (notification emails are skipped, password reset is unavailable, local sign-ups and email changes skip verification); `smtp_port` defaults to 465

### Security

- Revoke the provider OAuth grant (Google, GitHub, Slack, Linear, Todoist, TickTick) when an integration is disconnected or the account is deleted; a failed revocation is retried in the background until the provider accepts it
- Stop sending personal data to the tracing backend: no email address or passkey username in span fields, log messages or error messages, and an exporter-side filter redacts any email address left in spans / log bodies and drops the client IP (`http.client_ip`)
- Email-verification links now expire (`application.security.email_verification_token_validity_in_hours`, 24 h by default) and can only be used once
- Verify Slack webhook signatures and require explicit user consent in the OAuth2 authorization code flow
- Close IDOR / cross-tenant access paths: uniform 404 on integration-connection probes and blocked cross-tenant writes through the task third-party-item endpoint
- Harden authentication — IP-based rate limiting on auth/OAuth2 endpoints, atomic refresh-token rotation with reuse detection, and nonce/origin-bound, non-enumerable passkey ceremonies
- Stop leaking the Crisp HMAC signing key via `/api/front_config` (now `Cache-Control: private`), tighten CORS, and add `frame-ancestors` / `X-Frame-Options` clickjacking protection
- Run the Docker runtime stage as non-root on a pinned base image and pin third-party CI/CD actions to commit SHAs
- Check dependencies with cargo-deny (advisories, sources, bans) on every PR and main push, and let Dependabot update the Cargo workspace and the web npm package
- Update `quinn-proto` to 0.11.14 to fix RUSTSEC-2026-0037 (DoS via invalid QUIC transport parameters)
- Replace `typed_id` + `paste` crates with inline implementation to resolve RUSTSEC-2024-0436 (unmaintained `paste` crate)
- Downgrade `zip` from yanked 7.4.0 to 7.2.0 (resolves GH#133)
- Keep OAuth credentials and OIDC ID tokens out of traces: secret types no longer print their value, and the exporter redacts any JWT left in spans / log bodies
- Changing or resetting a password signs the user out of their other sessions and sends them a confirmation email
- Limit the length of user-supplied task, project and profile name fields, reject unknown fields in request bodies, and sanitize the HTML of GitHub, Google Calendar and Google Drive previews
- Per-user rate limits on syncs (10 per minute, shared by notifications and tasks) and bulk notification updates (30 per minute), on the REST API and the MCP tools, answered with 429 and `Retry-After`
- Refuse to start outside dev and test with the committed OAuth token encryption key or the placeholder database and SMTP passwords
- Send `X-Content-Type-Options: nosniff` and `Referrer-Policy: strict-origin-when-cross-origin` on every response, and default API responses to `Cache-Control: no-store` and `Content-Disposition: attachment` when the handler sets none
- Static content answers 405 with an `Allow` header to methods other than GET and HEAD
- Logging out revokes the session server-side; session checks fail closed with a 503 when Redis is unavailable, and the API no longer starts without Redis
- Name the session cookie `__Host-id` with explicit `Secure`, `HttpOnly` and `Path=/` attributes (signs every user out once)
- Server errors on the REST API and the MCP tools return a generic message with a correlation id instead of the error details, which are logged with the same id
- Redact one-time tokens, email addresses and JWTs from stdout logs, stop logging the OIDC state, nonce and authorization URL, and drop the OIDC callback code from browser storage once used
- Send the password-reset token in the request body (`POST /users/{user_id}/password-reset`) instead of the URL path
- Wrap email addresses in a `Pii<T>` type whose `Debug` output hides the value, so they cannot reach logs by accident
- New passwords must be 12 to 128 characters and must not be a common password, checked server-side; password forms show a strength meter
- Email users when a password reset completes, when a login method (password, passkey, Google) is added or removed, and when an email change is requested
- Changing the email address, adding or removing a login method, linking a Google account and creating an API token require a login less than 15 minutes old (`security.reauthentication_window_in_seconds`); the web app asks the user to confirm their identity with their password, passkey or Google account
- Allow only permissive dependency licenses (MPL-2.0 per crate) with cargo-deny, which now checks the whole workspace, API and web dependencies included
- Scan the API container image with Trivy weekly and whenever the image definition changes, failing on fixable HIGH and CRITICAL vulnerabilities

### Fixed

- Deleting a user now cancels their Stripe subscription immediately (and unlinks the Stripe customer, kept for invoices) before deleting local data; a Stripe failure aborts the deletion instead of leaving the user charged
- Deleting a user or a notification no longer fails with a foreign-key violation when Slack bridge pending actions reference it
- Persist MCP sessions to Redis so they survive multi-pod restarts
- Mark an integration connection as Failing when its OAuth refresh token is missing or rejected (`invalid_grant`)
- Preserve integration-connection context when updating its configuration
- Preserve precision of numeric-looking environment variables in the config loader
- Scope the `slack:list_emojis` Redis cache entry by workspace team id so custom emojis from one workspace are no longer served to users of other workspaces
- Completing a Slack-sourced task no longer fails when its Slack reaction was already removed
- Mark a Slack connection as Failing when Slack rejects its refresh token (`invalid_refresh_token` / `invalid_grant`)
- The toast shown after answering a Google Calendar invitation now matches the chosen answer
- Slack webhook events now reach Slack connections created before their workspace id was recorded (backfilled by the `slack backfill-team-id` command and on the next Slack sync)
- Convert every Google Calendar email into its invitation, including occurrences of recurring events and cancelled events
- Keep the text of an email readable in dark mode when its light background comes from a nested table
- A deleted GitHub discussion notification brought back by new activity only shows the replies posted after its deletion as new
- The add password and add passkey forms take the full card width instead of resizing with the password strength label

## 2026-03-17

### Added

- Internalize OAuth for Linear integration (replace Nango)
- Add Universal Inbox documentation website
- Add notifications and tasks deep linking
- Display cancelled calendar events
- Display calendar events recurrence
- Add Universal Inbox "Web Page" notification type (browser extension)
- Add button to delete all notifications
- Allow the notifications details panel to be moved to the bottom of the screen
- Add a new way to turn notifications into a task with default parameters
- Add Google Drive integration
- Prevent user registration from blck listed domains
- Users can now update their firstname, lastname and password
- Detect backend and frontend version mismatch and force reload the application

### Changed

- Make Universal Inbox UI mobile friendly
- Collapse read Google mails
- Add third party API call rate limiter
- Disable mandatory email verification
- Make notification and task details panel resizable
- Display notifications elasped time since last update
- Open links from notifications preview in a new tab
- Automatically delete a notification when the user is the author of the latest message
- Implement exponential backoff retries for failed synchronizations

### Fixed

- Convert calendar event times to event's timezone
- Display calendar event descriptions as HTML
- Prevent application freeze when third party API is slow
- Disable Linear issues to tasks synchronization by default
- Ignore keyboard shortcut with modifiers (Ctrl, Alt, ...) pressed

## 2025-05-03

### Added

- Synchronize one way (Slack => Universal Inbox) Slack mentions as notifications
- Add new keyboard shortcuts to control the preview pane
- Add Google Calendar Event invitations from Google Mail as a notification
- Support multiple authentication mechanisms (ie. local + Google)
- Support Passkey authentication
- Add notifications pagination, filtering and sorting

### Changed

- Sort synced tasks list
- Email from Google Mail are now fully rendered as HTML or plain text
- Refresh UI look & feel

### Fixed

- Fix Slack message retrieved in a thread
- Fix Slack user group ID resolution
- Increase Slack task title size limit
- Disable Todoist task search when not connected
- Fix Slack message format with missing new lines
- Consider API calls without change (304 status) as successful
- Fix Linear notification unsubscribe
- Deduplicate Linear issue notifications

## 2024-10-21

### Changed

- Resolve Slack user, channel and usergroup IDs while rendering a Slack message

### Fixed

- Render Slack messages with attachments with title and text
- Prevent triggering tasks & notifications synchronization concurrently
- Update Todoist task title when source title is updated

## 2024-10-14

### Added

- User profile page to create API keys
- Show message when reaching inbox zero
- Add notification kind filtering
- Display Linear notification reason
- Display Linear project updates
- Display Linear issue new comments
- Display Linear project on notification item
- Display Linear Project and Team icons
- Connect to Slack and receive "saved for later" (aka. "stars") events
- Add Slack "saved for later" as notifications
- 2 way sync Slack "saved for later" and Todoist tasks
- 2 way sync assigned Linear issues and Todoist tasks
- Render Slack messages from Slack blocks
- Track required vs registered OAuth scopes to suggest a reconnection if needed
- Add synced tasks page
- Synchronize Slack reacted messages as notifications or tasks

### Changed

- Use JWT token as access authorization (via a cookie or the `Authorization` header)
- Introduce ThirdPartyItem entity for Tasks source data
- Synchronize notifications and tasks on async workers
- Trigger notifications and tasks synchronization when user is active

### Fixed

- Increase the number of connection to Postgres in production
- Split the Todoist projects cache per user
- Trace user ID in logs and traces
- Fetch Slack message in a thread if any
- Handle Slack blocks in attachments
- Add `default_due_at` setting while syncing Linear assigned issues
- Add cache directive to task projects search endpoint
- Create new Todoist sink task if deleted

## [Initial Version] - 2024-01-27

### Added

- Support listing notifications from:
  - Github Pull Requests
  - Github Discussions
  - Linear Issues
  - Linear Projects
  - Google Mail
  - Todoist tasks
- Display preview of notifications
- Act on notifications
  - Open in Browser
  - Delete notification
  - Unsubscribe from notification
  - Snooze notification
  - Create a task from notification
  - Link notification to an existing task
- Act on tasks in the notification list
  - Complete task
