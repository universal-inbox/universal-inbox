# GitHub Integration

![GitHub integration configuration](images/github-config.png =750x center)

## Notification Synchronization

The GitHub integration imports your [GitHub notifications](https://github.com/notifications) into Universal Inbox, giving you a centralized place to manage all your GitHub activity.

### What Gets Synchronized

You receive GitHub notifications for various activities you're subscribed to:

- **Conversations**: Comments on issues, pull requests, or gists
- **Repository Activity**: Updates in repositories you watch
- **CI/CD**: Status updates from GitHub Actions workflows
- **Repository Content**: Issues, pull requests, releases, security alerts, and discussions (if enabled)

### Managing Your Subscriptions

To control which GitHub notifications you receive, visit GitHub's documentation on [managing your subscriptions](https://docs.github.com/en/account-and-profile/managing-subscriptions-and-notifications-on-github/managing-subscriptions-for-activity-on-github/managing-your-subscriptions).

## Limitations & Important Notes

```admonish note
Due to GitHub API limitations, notifications can only be marked as read, not truly deleted. When you delete a GitHub notification in Universal Inbox, it will still appear in your [GitHub notification inbox](https://github.com/notifications) but will be marked as read.
```

### Unsubscribing from Notifications

Universal Inbox allows you to [unsubscribe](../../quick_start/inbox_screen.md#notification-actions) from receiving future updates on issues, pull requests, and other GitHub items.

```admonish note
When you unsubscribe from a notification in Universal Inbox, the notification will remain in your [GitHub notification inbox](https://github.com/notifications) but will be marked as read. This is due to the same GitHub API limitations mentioned above.
```

## Available Actions

With the GitHub integration, you can perform these actions directly from Universal Inbox:

- View detailed notification content
- Mark notifications as read/unread
- Unsubscribe from future updates
- Convert notifications to tasks in your task management tool
