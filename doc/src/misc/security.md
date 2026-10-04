# Security & Privacy

Universal Inbox issues you a few different kinds of credentials and authorizations as you use the product. This page describes where each one lives, how to inspect it, and how to revoke it if you no longer need it.

## Authentication methods

You can sign in to Universal Inbox using any combination of the methods your administrator has enabled — typically a password, a passkey (WebAuthn), and/or Google Sign-In (OpenID Connect). Multiple methods can be linked to the same account so you can pick whichever is most convenient on a given device.

To manage your authentication methods, open your user profile and look for the **Authentication methods** card:

![Authentication methods on the profile page](images/security-auth-methods.png =750x center)

From this card you can:

- See which methods are currently linked
- Add a password if you only signed up with Google or a passkey
- Add a passkey to an existing account
- Remove a method (Universal Inbox always keeps at least one method linked so you cannot lock yourself out)

```admonish note
The set of methods you can add depends on what is enabled on your instance. Self-hosted operators configure this through the `[[application.security.authentication]]` blocks in the server config.
```

## Authorized OAuth clients

When you sign an external application into Universal Inbox via OAuth (for example, an MCP client like Claude Desktop, or a custom script using the [OAuth 2.1 flow](api_usage.md#oauth-21)), the authorization is recorded on the **Security** page under **Authorized OAuth2 clients**:

![Authorized OAuth clients on the Security page](images/security-oauth-clients.png =750x center)

For each authorized client you can see:

- The client's display name
- The scopes it was granted (`read`, `write`)
- When it was first authorized
- When it was last used

Use the per-row action to revoke a client's access. Once revoked, that client's refresh tokens are invalidated immediately and any access token will stop working at the next request.

```admonish tip
Universal Inbox's integrations (GitHub, Linear, Slack, Todoist, TickTick, Google Mail/Calendar/Drive) run their OAuth flows directly against the upstream provider. You can review and revoke Universal Inbox's access to each provider from that provider's own security settings, the same way you would for any other connected app.
```

## API keys

API keys are an alternative to OAuth for tools that do not implement the MCP authorization spec (for example, the [Raycast extension](raycast.md)). The Security page lists every key you have created, when it was last used, and lets you revoke any key you no longer need.

For details on creating and using API keys, see [API usage](api_usage.md).

## Third-party scripts

The web app loads two scripts from vendor servers:

| Script | Loaded from | Purpose |
| --- | --- | --- |
| Headway widget | `cdn.headwayapp.co` | Shows the product changelog |
| Crisp chat | `client.crisp.chat` | Support chat, only when support chat is configured on the instance |

The vendors manage these scripts and publish them at URLs with no version, so they change without notice. Neither script is pinned with [Subresource Integrity](https://developer.mozilla.org/en-US/docs/Web/Security/Subresource_Integrity): a pinned hash would stop working at the vendor's next update. This is an accepted risk, limited by the following controls:

- The Content Security Policy only allows scripts from the application itself and from these two vendor hosts. Network and frame access is limited the same way.
- Headway displays the changelog inside an iframe served from its own domain (`headway-widget.net`), isolated from the application page.
- The Crisp loader library is pinned to an exact version in the web app's lockfile. The chat loads only when an instance enables support chat.

If a vendor starts publishing versioned, immutable script URLs, we will pin those scripts with Subresource Integrity.
