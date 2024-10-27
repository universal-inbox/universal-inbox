# Slack

## Slack Message Types

Universal Inbox collects specific types of Slack messages based on your configuration:

- **Saved for Later Messages**: Messages you've saved for later in Slack
- **Emoji-Reacted Messages**: Messages you've reacted to with specific emoji (configurable)
- **Direct Mentions**: Messages where you're explicitly mentioned

```admonish note
Slack notification collection is customizable. See the [Slack integration settings](../../config/setup/slack.md) to specify which message types appear in Universal Inbox.
```

## Available Actions

### Actions on notifications

The following actions apply to all Slack notifications from the [Inbox screen](../../quick_start/inbox_screen.md):

#### View in Slack

- **Keyboard Shortcut**: `Enter`
- **Effect**: Opens the message directly in Slack

This action lets you view the full message context in Slack, where you can respond, add reactions, or interact with threads.

#### Delete

- **Keyboard Shortcut**: `d`
- **Effect in Universal Inbox**: Removes the notification from your inbox until the next reply in the thread
- **Effect in Slack**: Remove the "saved for later" status from the message or remove the reaction. It does not mark the message as read.

Use this action when you want to clear a notification from your Universal Inbox. The notification will reappear if there's a new reply in the thread for notifications from a Slack mention.

```admonish note
Due to Slack API limitations, the read status of a message cannot be changed in Slack. The "Delete" action will only update the status in Universal Inbox.
```

#### Unsubscribe

- **Keyboard Shortcut**: `u`
- **Effect in Universal Inbox**: Removes the notification from your inbox
- **Effect in Slack**: Remove the "saved for later" status from the message or remove the reaction. It does not mark the message as read, nor unsubscribe the Slack thread for notifications from Slack mentions.

```admonish note
Due to Slack API limitations, the read and subscription status of a message/thread cannot be changed in Slack. The "Unsubscribe" action will only update the status in Universal Inbox.
```

#### Snooze

- **Keyboard Shortcut**: `s`
- **Effect in Universal Inbox**: Temporarily hides the notification for a few hours
- **Effect in Slack**: No change in Slack

Use this when you need to defer handling a message until later.

#### Create Task

- **Keyboard Shortcut**: `p`
- **Effect in Universal Inbox**: Links notification to a newly created task and remove the notification from your inbox
- **Effect in Slack**: Remove the "saved for later" status from the message or remove the reaction. It does not mark the message as read.
- **Effect in Task Manager**: Creates a new task with a link to the Slack message

Ideal for converting a Slack message into an actionable task in your task management tool.

#### Link to Task

- **Keyboard Shortcut**: `l`
- **Effect in Universal Inbox**: Links notification to an existing task and remove the notification from your inbox
- **Effect in Slack**: Remove the "saved for later" status from the message or remove the reaction. It does not mark the message as read.
- **Effect in Task Manager**: Add a link to the Slack message in the task description

Use this when you already have a task related to this Slack message.
