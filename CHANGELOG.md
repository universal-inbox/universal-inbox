# Universal Inbox Changelog

## Unreleased

### Added

- Custom Slack reaction emoji picker in integration settings
  - Replaces the hardcoded 5-emoji dropdown with a searchable selector
  - Searches standard Unicode emojis first, then workspace custom emojis to fill up to the result limit
  - New endpoint `GET /integration-connections/{id}/slack/emojis/search?matches=…` scoped to a specific connection so multiple Slack connections per user remain supported
  - Graceful degradation: if the workspace emoji list cannot be fetched, the search still returns matching standard emojis
- Add Slack browser-extension bridge for 2-way sync (delete/unsubscribe actions)
  - Extension polls for queued actions and executes them through Slack's private API using the user's authenticated browser session
  - Credential validation: extension sends live `team_id` + `user_id` pairs matched against the integration connection
  - Three-state action lifecycle: Pending → Failed (with exponential backoff retry) → PermanentlyFailed
  - Bridge status displayed on integration card and in Extension settings tab with actionable error messages
  - Extension bridge enabled by default for new Slack integration connections
- Support multiple authentication methods per user account
  - Users can now add password, passkey, or Google authentication to an existing account
  - New "Authentication methods" card on the user profile page to manage auth methods
  - "Link Google account" button to associate a Google account with an existing user
  - Auth methods can be removed as long as at least one method remains
- Add MCP (Model Context Protocol) server for AI agent integration
  - Streamable HTTP transport at `/api/mcp` endpoint
  - Tools for notification and task management (list, get, act, bulk act, create task, sync)
  - OAuth 2.1 authentication with PKCE (Dynamic Client Registration, authorization code flow, refresh token rotation)
  - OAuth 2.0 Protected Resource Metadata and Authorization Server Metadata discovery endpoints
  - Origin header validation, per-user rate limiting, and token audience validation
  - Backward-compatible with existing API key authentication

### Security

- Update `quinn-proto` to 0.11.14 to fix RUSTSEC-2026-0037 (DoS via invalid QUIC transport parameters)
- Replace `typed_id` + `paste` crates with inline implementation to resolve RUSTSEC-2024-0436 (unmaintained `paste` crate)
- Downgrade `zip` from yanked 7.4.0 to 7.2.0 (resolves GH#133)

### Fixed

- Scope the `slack:list_emojis` Redis cache entry by workspace team id so custom emojis from one workspace are no longer served to users of other workspaces

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
