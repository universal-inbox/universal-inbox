# Slack Integration

The Slack integration for Universal Inbox provides multiple ways to track important Slack content and convert it into tasks. This integration helps you ensure that important messages don't get lost in the stream of Slack conversations.

## Supported Slack Features

Universal Inbox connects with Slack through three different mechanisms. For each mechanism, you can choose to either receive items as notifications or synchronize them as tasks in your task management tool.

### 1. Saved for Later Messages

![Slack "saved for later" integration configuration](images/slack-saved-for-later-config.png =750x center)

Slack's "Save for later" feature (bookmark icon) lets you mark messages for future reference. With this integration, these saved messages can automatically:
- Appear in your Universal Inbox notification feed
- Be converted to tasks in your task management tool

### 2. Message Reactions

![Slack reaction integration configuration](images/slack-reaction-config.png =750x center)

Specific emoji reactions can trigger task creation. When you react to a message with your designated emoji, Universal Inbox can:
- Appear in your Universal Inbox notification feed
- Be converted to tasks in your task management tool

### 3. Message Mentions

![Slack mention integration configuration](images/slack-mention-config.png =750x center)

Messages where you're mentioned (@username or @groupname) can be tracked in Universal Inbox, helping you:
- Keep track of requests and questions directed to you
- Ensure you don't miss important mentions across multiple channels

## Configuration Options

Each Slack integration component has its own settings:

### Saved for Later

You must choose one of the following synchronization methods:

- **Notification Synchronization**: Saved messages appear in your Universal Inbox notification feed
- **Task Synchronization**: Saved messages are synchronized as tasks in your task management tool
  - **Project Assignment**: Optionally assign tasks to a specific project
  - **Due Date**: Set a default due date for tasks created from saved messages

```admonish note
You cannot enable both Notification and Task synchronization at the same time for the same integration.
```

### Message Reactions

- **Reaction Emoji**: Select which emoji reaction will trigger synchronization

You must choose one of the following synchronization methods:

- **Notification Synchronization**: Messages with your chosen reaction appear in your Universal Inbox notification feed
- **Task Synchronization**: Messages with your chosen reaction are synchronized as tasks in your task management tool
  - **Project Assignment**: Optionally assign tasks to a specific project
  - **Due Date**: Set a default due date for tasks created from reactions

### Message Mentions

Unlike the other integration options, Message Mentions can only be synchronized as notifications:

- **Notification Synchronization**: Messages mentioning you appear in your Universal Inbox notification feed

This allows you to keep track of conversations where you're mentioned.

```admonish tip
You can enable any combination of these Slack integrations based on your workflow needs. For example, you might only want to use the "Saved for Later" feature without tracking mentions.
```

## Available Actions

With the Slack integration, you can:

- View saved messages, reactions, and mentions in one place
- Convert these items into tasks with proper due dates
- Complete tasks directly from Universal Inbox
- Access message context without having to search through Slack history
