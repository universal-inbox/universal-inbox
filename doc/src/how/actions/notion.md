# Notion

## Notification Types

Universal Inbox collects two kinds of Notion notifications:

- **Comment**: a discussion on a page or a block, with all its comments and the commented content
- **Mention**: a block of a page mentioning you

## Available Actions

The Notion public API cannot mark comments as read or unsubscribe from a discussion: every action below only
changes Universal Inbox, never Notion.

### Actions on notifications

The following actions apply to all Notion notifications from the [Inbox screen](../../quick_start/inbox_screen.md):

#### View in Notion

- **Keyboard Shortcut**: `Enter`
- **Effect**: Opens the discussion, or the block mentioning you, in Notion

#### Delete

- **Keyboard Shortcut**: `d`
- **Effect in Universal Inbox**: Removes the notification from your inbox until the next update
- **Effect in Notion**: No change in Notion

The notification comes back when there is a new comment in the discussion.

#### Unsubscribe

- **Keyboard Shortcut**: `u`
- **Effect in Universal Inbox**: Removes the notification from your inbox
- **Effect in Notion**: No change in Notion

Future comments in the discussion won't appear in Universal Inbox, unless one of them mentions you again.

#### Snooze

- **Keyboard Shortcut**: `s`
- **Effect in Universal Inbox**: Temporarily hides the notification for a few hours
- **Effect in Notion**: No change in Notion

#### Create Task

- **Keyboard Shortcut**: `p`
- **Keyboard Shortcut (with default task parameters)**: `t`
- **Effect in Universal Inbox**: Links notification to a newly created task and removes the notification from your
  inbox
- **Effect in Notion**: No change in Notion
- **Effect in Task Manager**: Creates a new task with a link to the discussion or the block

#### Link to Task

- **Keyboard Shortcut**: `l`
- **Effect in Universal Inbox**: Links notification to an existing task and removes the notification from your inbox
- **Effect in Notion**: No change in Notion
