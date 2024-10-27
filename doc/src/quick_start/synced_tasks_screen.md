# The Synced Tasks Screen

![The synced tasks screen](images/synced-tasks-screen.png =750x center)

## Overview

```admonish warning
Task synchronization features require a connected task management tool (like Todoist)
```

The Synced Tasks screen displays all tasks synchronized between your connected tools and your task management system. It follows the same dual-pane layout as the Inbox screen, but optimized specifically for task management.

### Tasks List (Left Pane)

Each task entry includes:
- **Source**: Origin of the task (Linear, Slack, etc.)
- **Type**: Format of the task (Linear issue, Slack message reaction, Slack "saved for later" message, etc.)
- **Title**: Main subject with contextual details
- **Indicators**: Additional information like author, priority, and other metadata
- **Timestamp**: When the task was last updated

```admonish tip
Task priority is indicated by the bookmark icon color:

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

The preview pane displays comprehensive details about the selected task, allowing you to view its content and context without switching applications.

## Task Lifecycle

Universal Inbox maintains continuous synchronization between three points: source tools, your task management tool, and the Universal Inbox interface.

### Automatic Task Creation

When these events occur, Universal Inbox automatically creates tasks in your task management tool:
- A Linear issue is assigned to you
- You save a Slack message for later
- You add a specific reaction to a Slack message

### Task Completion

Tasks are removed from the list when marked as complete in any of these places:
- The original source tool
- Your task management tool
- Universal Inbox

### Completing Tasks

**Mark as Complete**: The primary action available for tasks is completion. When you complete a task in Universal Inbox, this status is synchronized across all connected systems:
- The source tool (e.g., Linear issue will be closed)
- Your task management tool (e.g., Todoist task will be completed)
- Universal Inbox interface

### Configuration & Documentation

- **Setup Instructions**: For detailed configuration options, see [Linear Integration](../config/setup/linear.md) or [Slack Integration](../config/setup/slack.md).
- **Technical Details**: To learn more about the synchronization process, visit the [Synchronizing Tasks](../how/synchronizing_tasks.md) page.

## Keyboard Shortcuts

```admonish tip
Press `?` anytime to display available keyboard shortcuts for faster navigation and task management.
For the complete reference of all keyboard shortcuts, visit the [Keyboard Shortcuts](../misc/keyboard_shortcuts.md) page.
```
