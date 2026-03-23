# Universal Inbox with AI Agents

Universal Inbox can be used from AI agents through its remote MCP endpoint.

Universal Inbox MCP is implemented as a standard remote HTTP MCP server. MCP-capable clients such as Claude Desktop, Claude Code, and compatible ChatGPT environments handle the transport details automatically.

## MCP endpoint

Use your Universal Inbox instance URL with the `/mcp` path:

- Hosted Universal Inbox: `https://app.universal-inbox.com/mcp`
- Self-hosted Universal Inbox: `https://<your-instance>/mcp`

## Authentication

Universal Inbox MCP v1 uses API keys.

To create an API key:

1. Open the user profile screen.
2. Click **Create new API key**.
3. Copy the key and store it securely. You will not be able to see it again.

![User profile screen](images/user-profile.png =750x center)

## Claude Desktop and Claude Code

Claude-compatible MCP clients can connect directly to the Universal Inbox remote MCP endpoint. Configure:

- the MCP server URL: `https://<your-instance>/mcp`
- the `Authorization` header with `Bearer <your-api-key>`

No local bridge or extra service is required.

If you test the endpoint manually with a generic HTTP client, send:

- `Authorization: Bearer <your-api-key>`
- `Content-Type: application/json`
- `Accept: application/json, text/event-stream`

The MCP transport returns SSE-framed `data:` responses for POST requests, so generic HTTP tools must parse the JSON payload from the SSE event body.

## ChatGPT-compatible MCP setup

ChatGPT support depends on the MCP-capable mode or workspace available in your OpenAI account.

When your ChatGPT environment supports remote MCP servers:

- use the same MCP endpoint: `https://<your-instance>/mcp`
- authenticate with the same API key using `Authorization: Bearer <your-api-key>`

If your ChatGPT workspace does not yet expose remote MCP configuration, Universal Inbox MCP will not be available there until OpenAI enables that capability for your account.

## What the MCP server exposes

Universal Inbox MCP v1 is tools-only. It is designed for notification and task management, including:

- listing notifications and tasks
- reading a specific notification or task
- acting on notifications
- bulk notification actions
- creating a task from a notification
- synchronizing notifications and tasks on demand

Read tools do not trigger synchronization unless you explicitly ask for it. Write actions execute immediately.
