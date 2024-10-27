# Linear Integration

![Linear integration configuration](images/linear-config.png =750x center)

The Linear integration offers comprehensive support for both notifications and issue tracking, allowing you to manage your Linear workflow directly within Universal Inbox.

## Notification Synchronization

When enabled, Universal Inbox imports all your Linear notifications, including:

- **Issue Updates**: Changes to status, priority, or assignments
- **Project Changes**: Updates to projects you're involved with
- **Mentions**: When you're tagged or referenced
- **Comments**: Responses on issues you're subscribed to

These notifications mirror what you would see in the [Linear notifications inbox](https://linear.app/docs/inbox), but are now consolidated alongside notifications from your other tools in Universal Inbox.

## Issue Synchronization

A key benefit of the Linear integration is that issues assigned to you can be automatically synchronized as tasks in your connected task management tool (like Todoist), creating a seamless workflow between your issue tracker and task manager.

```admonish info
### Bidirectional Synchronization

Changes are automatically reflected in both systems:

- **Task Management → Linear**: Completing a task in your task management tool will mark the Linear issue as complete
- **Linear → Task Management**: Closing an issue in Linear will complete the associated task

For more information about how task synchronization works with Linear, see the [How It Works](../../how/actions/linear.md) page.
```

## Configuration Options

You can customize how Linear issues appear in your task management tool:

- **Project Assignment**: Automatically sort issues into a specific project
- **Due Date**: Set the default due date for synchronized issues

```admonish tip
Both project assignment and due date settings are optional. If left unconfigured, tasks will use the default settings from your task management tool.
```

## Available Actions

With the Linear integration, you can perform these actions directly from Universal Inbox:

- View detailed issue information
- Mark notifications as read/unread
- Convert notifications to tasks (in addition to automatic synchronization)
- Complete tasks and have the status reflected in Linear
