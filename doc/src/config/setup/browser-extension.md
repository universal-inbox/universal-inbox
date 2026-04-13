# Browser Extension

The Universal Inbox browser extension enhances the integration between your browser and Universal Inbox. It provides two key capabilities:

## Features

### 1. Send Web Pages as Notifications

Send any web page you're viewing to your Universal Inbox as a notification. This lets you capture interesting articles, documentation, or any web content for later processing.

### 2. Slack Bridge (2-Way Sync)

The browser extension bridges the gap between Universal Inbox and Slack's private API, enabling actions that aren't possible through Slack's public API:

- **Mark as Read**: When you delete a Slack thread notification in Universal Inbox, the extension marks the thread as read in Slack
- **Unsubscribe**: When you unsubscribe from a Slack thread notification, the extension unsubscribes you from the thread in Slack

#### How the Slack Bridge Works

```text
Universal Inbox                    Browser Extension                  Slack
     |                                   |                              |
     | 1. Delete/Unsubscribe             |                              |
     |    notification                   |                              |
     |                                   |                              |
     | 2. Queue pending action           |                              |
     |                                   |                              |
     |          3. Poll for actions      |                              |
     |<----------------------------------|                              |
     |                                   |                              |
     |          4. Return pending actions|                              |
     |---------------------------------->|                              |
     |                                   |                              |
     |                                   | 5. Execute via private API   |
     |                                   |----------------------------->|
     |                                   |                              |
     |          6. Report success/failure|                              |
     |<----------------------------------|                              |
```

1. You perform a delete or unsubscribe action on a Slack thread notification in Universal Inbox
2. Universal Inbox queues the action as a "pending action" (only when extension bridge is enabled)
3. The browser extension polls Universal Inbox every 30 seconds for pending actions
4. Universal Inbox returns any pending actions, matching them by Slack team ID
5. The extension executes the action using Slack's private API through your authenticated browser session
6. The extension reports success or failure back to Universal Inbox

#### Requirements

- The extension must be installed and running in a browser where you are logged into Slack
- The extension bridge must be enabled in the [Slack integration settings](slack.md) under the "Extension" tab
- The Slack workspace in the browser must match the workspace connected in Universal Inbox

#### Supported Actions

 Universal Inbox Action | Slack Effect |
:-: | :-: |
 Delete (thread notification) | Mark thread as read |
 Unsubscribe (thread notification) | Unsubscribe from thread |

## Installation

### Firefox

1. Download the extension from the [Universal Inbox releases page](https://github.com/universal-inbox/universal-inbox-extension/releases)
2. Open `about:addons` in Firefox
3. Click the gear icon and select "Install Add-on From File..."
4. Select the downloaded `.xpi` file

### Chrome

1. Download the extension from the [Universal Inbox releases page](https://github.com/universal-inbox/universal-inbox-extension/releases)
2. Open `chrome://extensions` in Chrome
3. Enable "Developer mode" in the top right
4. Click "Load unpacked" and select the extracted extension directory

## Configuration

After installation, open the extension options to configure:

1. **API URL**: Set to your Universal Inbox instance URL (defaults to `https://app.universal-inbox.com`)
2. **Slack Bridge**: Enable the bridge in the Slack integration settings under the "Extension" tab

## Troubleshooting

### Extension not detected

- Verify the extension is installed and enabled in your browser
- Check that you are logged into Slack in the same browser
- Reload the extension from the browser's extension management page

### Team credential mismatch

- Ensure you are logged into the correct Slack workspace in the browser
- The workspace must match the one connected in Universal Inbox's Slack integration

### Actions failing

- Check the browser console for error messages from the extension
- Verify your Slack session hasn't expired (try refreshing Slack in the browser)
- Check that the extension has the necessary permissions
