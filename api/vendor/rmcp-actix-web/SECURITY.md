# Security Considerations

## Authorization Header Forwarding

This transport layer forwards Authorization headers from HTTP requests to MCP services. This enables two distinct architectural patterns:

### Pattern 1: MCP Service Authentication (MCP-Compliant)
- The Authorization header authenticates the client to the MCP service
- The MCP service validates tokens intended for itself
- The MCP service uses separate credentials for any upstream API calls
- **This follows MCP specification requirements**

### Pattern 2: Token Passthrough Proxy (Non-Compliant)
- The Authorization header is forwarded through the MCP service to backend APIs
- The MCP service acts as a transparent proxy for authentication
- Backend APIs receive and validate the original client tokens
- **This violates MCP specification: "MCP servers MUST NOT pass through the token it received from the MCP client"**

## Security Implications

When using Pattern 2 (Token Passthrough):
- **Confused Deputy Risk**: Tokens meant for one service are used at another
- **Audience Validation**: Backend APIs MUST validate token audience claims
- **Token Scoping**: Clients MUST obtain tokens scoped for the backend API, not the MCP server

## Recommendations

1. **For MCP Services**: Validate tokens according to OAuth 2.1 Section 5.2
2. **For Proxy Implementations**: Document clearly that token passthrough violates MCP spec
3. **For Production Use**: Consider implementing proper OAuth delegation instead of passthrough

See [rmcp-openapi#67](https://gitlab.com/lx-industries/rmcp-openapi/-/issues/67) for detailed discussion.