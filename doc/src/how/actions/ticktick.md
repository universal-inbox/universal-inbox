# TickTick

## Overview

TickTick is one of two task managers Universal Inbox synchronizes with (the other is [Todoist](todoist.md)). Like Todoist, it serves as a central repository for tasks and as the synchronization hub for status updates flowing back to source tools.

## Integration Role

TickTick in Universal Inbox serves several key functions:

1. **Task Repository**: Acts as the central storage for tasks created from notifications
2. **Synchronization Hub**: Status updates flow between Universal Inbox, TickTick, and the source tool of each notification
3. **Task Creation Target**: When you create a task from a notification, you can pick TickTick as the destination (if Todoist is also connected, a task-manager picker appears)

## Available Actions

### Task Management

#### Complete Task

- **Keyboard shortcut**: `c`
- **Effect in Universal Inbox**: Marks the synchronized task or task associated to a notification as complete and removes the notification from your inbox
- **Effect in TickTick**: Completes the task in TickTick
- **Effect in Source Tool**: Updates the status in the original platform (e.g., completes a Linear issue, removes a Slack reaction)

This is the primary action for tasks, which synchronizes completion status across all platforms.
