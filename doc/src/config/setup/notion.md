# Notion Integration

The Notion integration brings Notion comments and page mentions into Universal Inbox as notifications.

## Connecting Notion

When you connect Notion, Notion shows its page picker: choose the pages (or teamspaces) Universal Inbox may read.
Universal Inbox only sees the pages you share with it. To share more pages later, open a page in Notion and add
Universal Inbox from its `•••` menu → **Connections**.

```admonish info
Universal Inbox only reads your Notion workspace. It never writes pages or comments.
```

## Configuration Options

- **Synchronize Notion comments as notifications**: comments in the discussions you take part in or are mentioned
  in.
- **Synchronize Notion page mentions as notifications**: mentions of you in the content of a page.

## Comments

A Notion discussion becomes one notification. It is notified when:
- someone mentions you in a comment;
- someone replies in a discussion you have written in;
- someone comments on a page or a block you created.

Each new comment updates the same notification and marks it unread again. When you reply in Notion yourself, the
notification is removed until someone else answers.

## Page Mentions

A block (a paragraph, a list item, a table row...) mentioning you with `@you` becomes a notification. Mentioning
yourself creates nothing. When the mention is removed from the block, or the block is deleted, the notification is
removed too.

## Synchronization

Notion sends comments and page changes to Universal Inbox as they happen, so notifications arrive within seconds.
A periodic synchronization catches part of what a webhook may miss:
- every hour, new replies in the discussions already in Universal Inbox with activity in the last 14 days (the 30
  most recent ones);
- at each synchronization, page mentions added or removed since the previous one.

New discussions, deleted discussions and deleted blocks are only picked up through webhooks. Notion retries a webhook
that Universal Inbox did not receive for about 24 hours. The Notion API cannot find new comments afterwards: adding a
comment does not change a page's last edited time, and no endpoint lists a workspace's comments. A new discussion whose
webhook never arrives is not notified.

## Available Actions

With Notion notifications in Universal Inbox, you can:

- View the whole discussion and the commented block, or the block mentioning you
- Delete, unsubscribe from or snooze the notification
- Convert a notification into a task in your task management tool

See [Notion actions](../../how/actions/notion.md) for details.
