# Database TLS

This page is for operators of a self-hosted Universal Inbox instance.

Universal Inbox connects to PostgreSQL with the `[database]` settings. Use TLS whenever the connection crosses a network you do not fully control.

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `use_tls` | `false` | Enforce TLS. The connection fails if the server refuses TLS. |
| `tls_mode` | `verify-full` | `verify-full`, `verify-ca` or `require`. Only valid with `use_tls = true`. |
| `tls_root_cert_path` | system trust store | PEM root certificate used by `verify-ca` and `verify-full`. Only valid with `use_tls = true`. |

Each setting can also be set with an environment variable, for example `UNIVERSAL_INBOX__DATABASE__USE_TLS=true`.

## TLS modes

- `verify-full` (default with `use_tls = true`): encrypt the connection, verify the server certificate is signed by a trusted authority, and verify it matches `host`. Use it across untrusted networks.
- `verify-ca`: like `verify-full`, without the hostname check.
- `require`: encrypt the connection and fail if the server does not support TLS. The server certificate is not verified, so this protects against passive eavesdropping, not against an active man-in-the-middle. Choose it only when the server certificate cannot be verified, for example a managed PostgreSQL service with a certificate your system does not trust.

For `verify-ca` and `verify-full`, set `tls_root_cert_path` when the server uses a private authority. Without it, the system trust store is used.

Upgrading: an instance that ran with `use_tls = true` before `tls_mode` existed now verifies the certificate and hostname. Set `tls_mode = "require"` to keep the previous encryption-only behaviour until the certificate is trusted.

## `use_tls = false`

With `use_tls = false`, TLS is opportunistic: the client tries TLS first and falls back to plaintext if the server refuses it. Anyone who can intercept the connection can force that refusal and read the traffic. Keep `use_tls = false` only for local or otherwise trusted connections.

## Invalid combinations

Universal Inbox refuses to start when a TLS setting would be silently ignored:

- `tls_mode` or `tls_root_cert_path` with `use_tls = false`
- `tls_root_cert_path` with `tls_mode = "require"`, which does not verify the certificate and would ignore it
