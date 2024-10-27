# Google Mail Integration

![Google Mail integration configuration](images/google-mail-config.png =750x center)

## Email Synchronization

The Google Mail integration allows you to selectively import email threads as notifications in Universal Inbox, giving you a streamlined way to manage important emails alongside your other notifications.

```admonish info
### Email Triage, Not Replacement

Universal Inbox complements your email client rather than replacing it. You cannot reply to emails directly from Universal Inbox.

Universal Inbox serves as a powerful triage tool to help you:
- **Prioritize**: Quickly identify important emails among other notifications
- **Review**: Decide which messages need immediate attention
- **Act**: Determine appropriate actions (delete, snooze, or convert to a task)
- **Track**: Convert emails into tasks in your task management tool

Continue using your preferred email client alongside Universal Inbox for complete email functionality.
```

## How It Works

The Google Mail integration offers a selective approach to email management:

### Label-Based Filtering

Universal Inbox only synchronizes email threads that have a specific Google Mail label. This allows you to create Google Mail filters to automatically select which emails you want to manage in Universal Inbox, such as:

- Notifications from third-party services not directly supported by Universal Inbox
- Important emails where you are the direct recipient
- Messages requiring follow-up or action
- Specific categories of messages you want to track alongside other notifications

### Thread Consolidation

Each email thread appears as a single notification in Universal Inbox, regardless of how many individual emails the thread contains. This reduces clutter and provides a cleaner view of your communications.

## Configuration Options

- **Google Mail Label to Synchronize**: Select which labeled emails will be used to synchronize with Universal Inbox

```admonish tip
For best results:
1. Create a dedicated label in Google Mail like `Universal-Inbox`
2. Set up Google Mail filters to automatically apply this label to important messages
3. Select this label in Universal Inbox settings

This approach gives you precise control over which emails appear in Universal Inbox.
```

## Available Actions

With Google Mail integration, you can perform these actions directly from Universal Inbox:

- View email thread content
- Delete threads (mark as read and archive in Google Mail)
- Convert emails into tasks in your task management tool
- Snooze emails for later handling
