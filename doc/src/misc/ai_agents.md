# Universal Inbox with AI Agents

Universal Inbox can be used from AI agents through its remote MCP endpoint.

Universal Inbox MCP is implemented as a standard remote HTTP MCP server. MCP-capable clients such as Claude Desktop, Claude Code, and compatible ChatGPT environments handle the transport details automatically.

## MCP endpoint

Use your Universal Inbox instance URL with the `/api/mcp` path:

- Hosted Universal Inbox: `https://app.universal-inbox.com/api/mcp`
- Self-hosted Universal Inbox: `https://<your-instance>/api/mcp`

## Authentication

Universal Inbox MCP supports two authentication methods:

### OAuth 2.1 (recommended)

Universal Inbox implements an OAuth 2.1 authorization server following the [MCP authorization specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization). MCP clients that support OAuth (such as Claude Desktop and Claude Code) can authenticate automatically using the standard OAuth flow.

The server provides discovery endpoints for automatic configuration:

- **Protected Resource Metadata**: `GET /.well-known/oauth-protected-resource`
- **Authorization Server Metadata**: `GET /.well-known/oauth-authorization-server`

The OAuth flow uses:

- **Dynamic Client Registration** at `POST /api/oauth2/register`
- **Authorization Code with PKCE (S256)** at `GET /api/oauth2/authorize`
- **Token exchange and refresh** at `POST /api/oauth2/token`

Access tokens are scoped with `read` and `write` permissions. Refresh tokens are rotated on each use for security.

MCP clients that support the MCP authorization spec will handle this flow automatically — no manual configuration is needed beyond providing the MCP server URL.

### API keys (legacy)

API keys are still supported for clients that do not support OAuth.

To create an API key:

1. Open the user profile screen.
2. Click **Create new API key**.
3. Copy the key and store it securely. You will not be able to see it again.

![User profile screen](images/user-profile.png =750x center)

## Claude Desktop and Claude Code

Claude-compatible MCP clients can connect directly to the Universal Inbox remote MCP endpoint. Configure the MCP server URL: `https://<your-instance>/api/mcp`

If your client supports OAuth (MCP spec 2025-11-25), authentication is handled automatically through the OAuth flow.

For clients that do not support OAuth, configure:

- the MCP server URL: `https://<your-instance>/api/mcp`
- the `Authorization` header with `Bearer <your-api-key>`

No local bridge or extra service is required.

### Manual testing

If you test the endpoint manually with a generic HTTP client, send:

- `Authorization: Bearer <your-token>` (OAuth access token or API key)
- `Content-Type: application/json`
- `Accept: application/json, text/event-stream`

The MCP transport returns SSE-framed `data:` responses for POST requests, so generic HTTP tools must parse the JSON payload from the SSE event body.

## ChatGPT-compatible MCP setup

ChatGPT support depends on the MCP-capable mode or workspace available in your OpenAI account.

When your ChatGPT environment supports remote MCP servers:

- use the MCP endpoint: `https://<your-instance>/api/mcp`
- if OAuth is supported, authentication is automatic
- otherwise, authenticate with an API key using `Authorization: Bearer <your-api-key>`

If your ChatGPT workspace does not yet expose remote MCP configuration, Universal Inbox MCP will not be available there until OpenAI enables that capability for your account.

## What the MCP server exposes

Universal Inbox MCP is tools-only. It is designed for notification and task management, including:

- listing notifications and tasks
- reading a specific notification or task
- acting on notifications
- bulk notification actions
- creating a task from a notification
- synchronizing notifications and tasks on demand

Read tools do not trigger synchronization unless you explicitly ask for it. Write actions execute immediately.
