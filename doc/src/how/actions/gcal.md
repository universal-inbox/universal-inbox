# Google Calendar

## Calendar Notification Types

Universal Inbox collects specific types of Google Calendar notifications:

- **Meeting Invitations**: New calendar invitations requiring your response
- **Meeting Updates**: Changes to meetings you're invited to
- **Cancellations**: Notifications about canceled meetings

```admonish note
Google Calendar integration focuses primarily on invitation management, helping you respond to meeting requests without leaving Universal Inbox.
```

## Available Actions

### Actions on notifications

The following actions apply to all Google Calendar notifications from the [Inbox screen](../../quick_start/inbox_screen.md):

#### View in Calendar

- **Keyboard Shortcut**: `Enter`
- **Effect**: Opens the calendar event in Google Calendar

This action lets you view the full event details directly in Google Calendar, where you can see the complete attendee list, access video conferencing links, or view your full schedule.

#### Delete

- **Keyboard Shortcut**: `d`
- **Effect in Universal Inbox**: Removes the notification from your inbox until the next update
- **Effect in Google Calendar**: No change in Google Calendar

Use this action when you want to clear a notification from your Universal Inbox without affecting its status in Google Calendar. The notification will reappear if the event is updated.

#### Unsubscribe

- **Keyboard Shortcut**: `u`
- **Effect in Universal Inbox**: Removes the notification from your inbox
- **Effect in Google Calendar**: No direct effect in Google Calendar, but future updates to this event won't appear in Universal Inbox

This action helps reduce notification noise by preventing future updates about this event from appearing in Universal Inbox.

#### Snooze

- **Keyboard Shortcut**: `s`
- **Effect in Universal Inbox**: Temporarily hides the notification for a few hours
- **Effect in Google Calendar**: No change in Google Calendar

Use this when you need to defer handling an invitation until later.

#### Create Task

- **Keyboard Shortcut**: `p`
- **Effect in Universal Inbox**: Links notification to a newly created task and remove the notification from your inbox
- **Effect in Google Calendar**: No change in Google Calendar
- **Effect in Task Manager**: Creates a new task with a link to the calendar event

Ideal for creating follow-up tasks related to calendar events.

#### Link to Task

- **Keyboard Shortcut**: `l`
- **Effect in Universal Inbox**: Links notification to an existing task and remove the notification from your inbox
- **Effect in Google Calendar**: No change in Google Calendar

Use this when you already have a task related to this calendar event.

### Calendar-Specific Actions

#### Accept Invitation

- **Keyboard Shortcut**: `y`
- **Effect in Universal Inbox**: Updates invitation status indicator to "Accepted" and remove the notification from your inbox
- **Effect in Google Calendar**: Marks you as "Going" for the event

Quickly accept meeting invitations directly from Universal Inbox.

#### Maybe/Tentative

- **Keyboard Shortcut**: `m`
- **Effect in Universal Inbox**: Updates invitation status indicator to "Maybe" and remove the notification from your inbox
- **Effect in Google Calendar**: Marks you as "Maybe" for the event

Indicates that you might attend the meeting but aren't committing yet.

#### Decline Invitation

- **Keyboard Shortcut**: `n`
- **Effect in Universal Inbox**: Updates invitation status indicator to "Declined" and remove the notification from your inbox
- **Effect in Google Calendar**: Marks you as "Not Going" for the event

Decline meetings that you can't or don't want to attend.
