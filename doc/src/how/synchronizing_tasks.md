# Synchronizing tasks

## Overview

Task synchronization is a core feature of Universal Inbox, enabling bidirectional sync between your task management tool (such as Todoist) and other connected tools (e.g., Linear, Slack). This ensures that tasks remain consistent across all tools in your workflow.

## Synchronization Mechanisms

### OAuth Authorization

When you connect an integration (GitHub, Linear, Google Mail, Slack), Universal Inbox establishes a secure connection using OAuth:

1. You authorize Universal Inbox to access your account on the respective tool
2. The tool provides access tokens that Universal Inbox securely stores
3. These tokens are used to fetch notifications on your behalf

### Bidirectional Synchronization

Universal Inbox maintains real-time consistency between tools through:

1. **Source to Task Manager**: When tasks are created or updated in source tools (Linear, Slack), changes are synchronized to your task management tool
2. **Task Manager to Source**: When tasks are completed or updated in your task management tool, changes are reflected back in the source tools

### Synchronization Frequency

Task synchronization occurs through:

- **Automatic Background Sync**: Occurs every few minutes while you're logged in
- **Manual Refresh**: Triggered when you connect or re-connect an integration
 
### Tool-Specific Synchronization

#### Linear Integration

When synchronizing with Linear:

- Assigned issues in Linear appear as tasks in your task manager
- Completing a task in your task manager marks the Linear issue as completed

#### Slack Integration

Unlike other integrations, Slack uses a real-time webhook system that delivers events to Universal Inbox as they occur.

When synchronizing with Slack:

- Messages marked as "saved for later" appear as tasks in your task manager
- Messages with specific reactions appear as tasks in your task manager
- Completing a task in your task manager removes the saved status or reaction in Slack

```admonish tip
Specify which emoji reactions should trigger task creation in the [Slack integration settings](../config/setup/slack.md)
```

### Notification to Task Conversion

When you convert notifications into tasks:

1. A new task is created in your task management tool
2. The task includes a link back to the original notification source
3. The notification is marked as associated with this task in Universal Inbox

### Data Mapping

To ensure accurate synchronization, Universal Inbox maps fields between different platforms:

- **Task Title**: Maintained across platforms with source context
- **Task Status**: Completion status is synchronized bidirectionally
- **Task Priority**: When available, priority levels are mapped between systems
- **Task Details**: Description, notes, and metadata are preserved

## Task Lifecycle

Synchronized tasks follow a consistent lifecycle:

1. **Creation**: Tasks are created in source tools or by converting notifications
2. **Synchronization**: Tasks are synchronized to your task management tool
3. **Updates**: Changes to task properties (priority, description) are synchronized bidirectionally. Due date is not updated after the creation of the task to allow you to keep your own organization of tasks.
4. **Completion**: When marked as complete in either system, the completion status is synchronized.
