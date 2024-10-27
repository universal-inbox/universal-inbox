# GitHub

## GitHub Notification Types

Universal Inbox collects several types of GitHub notifications:

- **Issue**: Updates on issues you're assigned, mentioned in, or watching
- **Pull Request**: Review requests, comments, approvals, and mentions
- **Discussion**: Updates on discussions you've participated in
- **Repository Invitation**: Invitations to collaborate on repositories
- **Security Alert**: Security vulnerabilities in repositories you maintain
- **Workflow Run**: CI/CD workflow completion notifications

## Available Actions

### Actions on notifications

The following actions apply to all GitHub notifications from the [Inbox screen](../../quick_start/inbox_screen.md):

#### View in GitHub

- **Keyboard Shortcut**: `Enter`
- **Effect**: Opens the notification source in GitHub

This action lets you view the full context of the notification directly in GitHub, where you can respond, review code, or participate in discussions.

#### Delete

- **Keyboard Shortcut**: `d`
- **Effect in Universal Inbox**: Removes the notification from your inbox until the next update
- **Effect in GitHub**: The notification is marked as read (it is not deleted due to Github API limitations)

Use this action when you want to clear a notification from your Universal Inbox and GitHub. The notification will reappear if updated in GitHub.

#### Unsubscribe

- **Keyboard Shortcut**: `u`
- **Effect in Universal Inbox**: Removes the notification from your inbox
- **Effect in GitHub**: Unsubscribes you from the underlying issue or discussion, preventing future notifications. It can still be re-subscribed to if you are directly pinged in the issue or discussion.

This action helps reduce notification noise by unsubscribing you from conversations that aren't relevant to your work.

#### Snooze

- **Keyboard Shortcut**: `s`
- **Effect in Universal Inbox**: Temporarily hides the notification for a few hours
- **Effect in GitHub**: No change in GitHub

Use this when you need to defer handling a notification until later.

#### Create Task

- **Keyboard Shortcut**: `p`
- **Effect in Universal Inbox**: Links notification to a newly created task and remove the notification from your inbox
- **Effect in GitHub**: Mark the notification as read
- **Effect in Task Manager**: Creates a new task with a link to the GitHub item

Ideal for converting a GitHub notification into a task in your task management tool.

#### Link to Task

- **Keyboard Shortcut**: `l`
- **Effect in Universal Inbox**: Links notification to an existing task and remove the notification from your inbox
- **Effect in GitHub**: Mark the notification as read
- **Effect in Task Manager**: Add a link to the GitHub item in the task description

Use this when you already have a task related to this GitHub notification.
