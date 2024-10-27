# Connecting Your Tools

## Getting Started with Universal Inbox

To begin centralizing your notifications and tasks in Universal Inbox, you'll need to connect your preferred tools. This guide walks you through the simple connection process.

### Initial Setup

1. On your first login, you'll automatically see the [Settings screen](https://app.universal-inbox.com/settings)
2. This screen displays all available tool integrations

![The settings screen](images/first-start-settings-screen.png =750x center)

### Connecting Tools

To connect a tool:

1. Find your desired tool on the [Settings screen](https://app.universal-inbox.com/settings)
2. Click the "Connect" button
3. A new window will open showing either:
   - The tool's login page (if you're not already logged in)
   - An authorization request for Universal Inbox
4. The window closes automatically once connection is established

```admonish note
Each integration uses a secure OAuth authorization flow, ensuring your credentials are never directly shared with Universal Inbox.
```

After connecting a tool, Universal Inbox automatically fetches notifications and displays them on your [Inbox screen](inbox_screen.md).

```admonish tip
Connecting a task management tool (like Todoist) is highly recommended as it enables core features:

- Converting notifications into tasks
- Synchronizing tasks across all your connected tools

![Todoist integration settings](../config/setup/images/todoist-config.png =700x center)
```

## Next Steps

After connecting your tools:

- View all your notifications in the [Inbox screen](inbox_screen.md)
- Manage synchronized tasks in the [Synced Tasks screen](synced_tasks_screen.md)
- Configure individual integrations in [Integration Settings](../config/setup/)
