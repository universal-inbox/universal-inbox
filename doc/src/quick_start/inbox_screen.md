# The Inbox Screen

![The Inbox screen](images/inbox-screen.png =750x center)

## Overview

The Inbox screen is your central hub for managing all synchronized notifications from your connected tools.

### Notifications List (Left Pane)

Each notification entry includes:
- **Source**: Where the notification originated (GitHub, Linear, Google Mail, Slack, etc.)
- **Type**: The format of content (Linear issue, Slack message, GitHub discussion, etc.)
- **Title**: Main subject with contextual details
- **Indicators**: Additional information such as author names, notification reasons, PR review status, etc.
- **Timestamp**: When the notification was last updated

```admonish tip
The bookmark icon shows when a notification is linked to a task. The color indicates priority level:

{:.icon-text}
![task bookmark](images/task-bookmark-gray.png =x20) Default/Low priority

{:.icon-text}
![task bookmark](images/task-bookmark-yellow.png =x20) Medium priority

{:.icon-text}
![task bookmark](images/task-bookmark-orange.png =x20) High priority

{:.icon-text}
![task bookmark](images/task-bookmark-red.png =x20) Urgent priority
```

### Preview Pane (Right Side)

The preview pane displays comprehensive details about the selected notification, allowing you to view content without leaving Universal Inbox.

## Managing Notifications

Universal Inbox doesn't just collect your notifications, it empowers you to take action directly from the interface.

```admonish info
Below are the key actions available for your notifications.
For detailed information about how these actions affect the source tools, see the [Actions by Integration](../how/actions/index.html) guide.
```

### Notification Actions

{:.icon-text}
![delete button](images/delete-button.png =x30) Delete: Remove the notification until its next update

{:.icon-text}
![unsubscribe button](images/unsubscribe-button.png =x30) Unsubscribe: Permanently silence this notification and all its future updates

{:.icon-text}
![snooze button](images/snooze-button.png =x30) Snooze: Temporarily hide the notification to handle it at a later time

### Task Management actions

```admonish warning
To use the task management features below, you must first connect a task management tool in Settings
```

#### Create Task

![create task modal](images/create-task-modal.png =350x center)

{:.icon-text}
![create task button](images/create-task-button.png =30x30) Convert to Task: Transform your notification into an actionable task in your task management tool. After clicking this button, you can customize the task details (title, project, due date, priority) before creation.

{:.icon-text}
![create task with defaults button](images/create-task-with-defaults-button.png =30x30) Convert to Task with default settings: Transform your notification into an actionable task in your task management tool. Default task details are automatically set from Todoist settings.

#### Link to Task

![link to task modal](images/link-to-task-modal.png =350x center)

{:.icon-text}
![link to task button](images/link-to-task-button.png =30x30) Link to Existing Task: Associate your notification with a task you've already created. The form allows you to search for and select the appropriate task to establish the link.

### Type-Specific Actions

Different notification types offer specialized actions relevant to their content:

{:.icon-text}
![Google Calendar action buttons](images/google-calendar-action-buttons.png =x30) Answer Invitation: Accept or decline Google Calendar invitations directly within Universal Inbox without switching applications.

## Keyboard Shortcuts

```admonish tip
Press `?` anytime to display available keyboard shortcuts for faster navigation and actions.
For the complete reference of all keyboard shortcuts, visit the [Keyboard Shortcuts](../misc/keyboard_shortcuts.md) page.
```
