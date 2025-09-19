# Integration Setup

Each connected tool has specific configuration options to customize how it synchronizes with Universal Inbox.

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

- **Not Yet Synchronized**: Initial state before the first synchronization occurs
  ![not yet synchronized](images/not-yet-synchronized-sync-status.png =x20)

- **Successfully Synchronized**: Data has been synchronized without issues
  ![successfully synchronized](images/successful-sync-status.png =x20)

- **Synchronization Failed**: An error occurred during synchronization
  ![synchronization failed](images/failed-sync-status.png =x20)
  
  If errors persist, try disconnecting and reconnecting the integration. If problems continue, please contact [support](mailto:support@universal-inbox.com).

## Tool-Specific Configuration

For detailed setup instructions for each tool, select the appropriate guide:

- [GitHub](github.md)
- [Linear](linear.md)
- [Slack](slack.md)
- [Google Mail](gmail.md)
- [Google Calendar](gcal.md)
- [Google Drive](gdrive.md)
- [Todoist](todoist.md)
