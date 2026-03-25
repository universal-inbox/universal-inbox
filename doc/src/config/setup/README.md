# Integration Setup

Each connected tool has specific configuration options to customize how it synchronizes with Universal Inbox.

```admonish info
All integrations authorize Universal Inbox directly through the provider's own OAuth flow. You can review and revoke the apps you have authorized from the [Security & Privacy](../../misc/security.md) page, and from each upstream provider's own security settings.
```

## Connection Status

Your integrations will display one of these connection states:

- **Disconnected**: The integration is available but not yet connected
  ![disconnected integration](images/github-disconnected.png =400x)

- **Connected**: The integration is successfully connected and authorized
  ![connected integration](images/github-config.png =400x)

- **Needs Reconnection**: The integration is missing required authorizations. This typically happens when Universal Inbox adds new features that require additional permissions.
  ![integration needing reconnection](images/github-missing-oauth-scopes.png =400x)

## Synchronization Status

Once connected, each integration displays its current synchronization state:

- **Not Yet Synchronized**: Initial state before the first synchronization occurs.
- **Successfully Synchronized**: Data has been synchronized without issues.
- **Synchronization Failed**: An error occurred during synchronization. If errors persist, try disconnecting and reconnecting the integration. If problems continue, please contact [support](mailto:support@universal-inbox.com).

## Tool-Specific Configuration

For detailed setup instructions for each tool, select the appropriate guide:

- [GitHub](github.md)
- [Linear](linear.md)
- [Slack](slack.md)
- [Google Mail](gmail.md)
- [Google Calendar](gcal.md)
- [Google Drive](gdrive.md)
- [Todoist](todoist.md)
- [TickTick](ticktick.md)
