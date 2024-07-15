# Universal Inbox Changelog

## [Unreleased]

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
