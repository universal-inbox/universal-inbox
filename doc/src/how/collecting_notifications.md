# Collecting notifications

## Overview

Universal Inbox centralizes notifications from various tools into a unified interface. This process involves several steps to securely fetch, normalize, and display notifications from different sources.

## Notification Collection Mechanism

### OAuth Authorization

When you connect an integration (GitHub, Linear, Google Mail, Slack), Universal Inbox establishes a secure connection using OAuth:

1. You authorize Universal Inbox to access your account on the respective tool
2. The tool provides access tokens that Universal Inbox securely stores
3. These tokens are used to fetch notifications on your behalf

### Synchronization Frequency

Synchronization happens through two methods:

1. **Automatic Background Sync**: Occurs every few minutes while you're logged in
2. **Manual Refresh**: Triggered when you connect or re-connect an integration.

### Integration-Specific Collection

#### Slack

Unlike other integrations, Slack uses a real-time webhook system that delivers events to Universal Inbox as they occur. This results in faster notification delivery compared to the scheduled synchronization used by other integrations.

## Notification Lifecycle

After collection, notifications become part of the Universal Inbox workflow:

1. **Initial Collection**: Notification appears in your inbox
2. **User Action**: You can delete, snooze, unsubscribe, or convert to a task
3. **Updates**: If the source notification is updated, Universal Inbox refreshes its content
4. **Resolution**: When a notification is handled in its source platform or through Universal Inbox actions
