# Todoist

## Overview

Todoist integration is a central component of Universal Inbox, serving as the primary task management system. Rather than simply collecting notifications from Todoist, this integration enables bidirectional synchronization of tasks between Todoist and other connected tools.

## Integration Role

Todoist in Universal Inbox serves several key functions:

1. **Task Repository**: Acts as the central storage for all tasks across tools
2. **Synchronization Hub**: Enables bidirectional sync between tasks and source tools
3. **Task Creation Target**: Receives tasks created from notifications

## Available Actions

### Task Management

#### Complete Task

- **Keyboard shortcut**: `c`
- **Effect in Universal Inbox**: Marks the synchronized task or task associated to a notification as complete and remove the notification from your inbox
- **Effect in Todoist**: Completes the task in Todoist
- **Effect in Source Tool**: Updates the status in the original platform (e.g., completes a Linear issue, removes a Slack "saved for later" status)

This is the primary action for tasks, which synchronizes completion status across all platforms.
