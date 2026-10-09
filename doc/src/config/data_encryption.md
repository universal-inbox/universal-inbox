# Data Encryption at Rest

This page is for operators of a self-hosted Universal Inbox instance.

Universal Inbox encrypts the most sensitive data it stores in PostgreSQL, so a leaked database dump or backup does not expose it on its own. The encryption key never goes to the database: it is part of the server configuration.

## What is encrypted

**Encrypted** (AES-256-GCM):

- Content synced from your tools: Slack messages, emails, calendar events, GitHub and Linear issues, Google Drive comments, Todoist and TickTick tasks
- Task descriptions
- User email addresses (account and pending email change) and the email addresses of the connected Google accounts, stored with a keyed hash (blind index) to look them up
- OAuth access and refresh tokens of the connected tools, and raw OAuth token responses
- OpenID Connect ID tokens of users signed in with Google

**Hashed**:

- Email validation, password reset and email change tokens, OAuth authorization codes (SHA-256)
- API keys and OAuth refresh tokens issued by Universal Inbox (SHA-256)
- Passwords (Argon2id)

**Not encrypted**, because they are used to search, filter and sort:

- Notification and task titles, task projects and tags
- User names
- Identifiers of items in your tools, statuses and dates

Email addresses are looked up (login, password reset, duplicate accounts) through a blind index: an HMAC-SHA256 of the lowercased address keyed with a dedicated secret. Email lookups are therefore case-insensitive.

Values are encrypted with AES-256-GCM, each one with a random nonce, and bound to their table, column and row: an encrypted value copied to another row cannot be decrypted. Each encrypted value records the id of the key that encrypted it, which is what makes key rotation possible.

## Configuring the key

Generate a key:

```sh
openssl rand -hex 32
```

Set it with environment variables (`docker/universal-inbox.env` for Docker deployments):

```sh
UNIVERSAL_INBOX__DATA_ENCRYPTION__ACTIVE_KEY_ID=1
UNIVERSAL_INBOX__DATA_ENCRYPTION__KEYS__1="<generated key>"
```

Also generate and set the blind index key (required, the server refuses to start without it):

```sh
UNIVERSAL_INBOX__DATA_ENCRYPTION__BLIND_INDEX_KEY="<another generated key>"
```

It is independent from the encryption keys and is **not** rotated with them: changing it would make every stored email address unfindable. Keep it as long as the instance lives.

```admonish warning
Store the keys in your secret manager, **not next to the database backups**: a backup and its key together expose the encrypted data.

Losing the key means losing the encrypted data. Content synced from your tools is synced again, but task descriptions written in Universal Inbox only, and OAuth connections (they must be reconnected), are lost.
```

```admonish note title="Upgrading an existing instance"
Set `UNIVERSAL_INBOX__DATA_ENCRYPTION__BLIND_INDEX_KEY` before deploying the version encrypting email addresses. Its migration also refuses to run while two accounts use the same email address with a different case: change one of them first.

Instances configured before data encryption existed only have `UNIVERSAL_INBOX__OAUTH2__TOKEN_ENCRYPTION_KEY`, the key encrypting OAuth tokens. When no `UNIVERSAL_INBOX__DATA_ENCRYPTION__KEYS__<id>` is set, that key is used as key `1`, so no configuration change is required. To move to the new settings, set `UNIVERSAL_INBOX__DATA_ENCRYPTION__KEYS__1` to the value of `UNIVERSAL_INBOX__OAUTH2__TOKEN_ENCRYPTION_KEY`.
```

## Encrypting existing data

Data stored before its column was encrypted is encrypted by:

```sh
universal-inbox data-encryption encrypt-plaintext
```

The container entrypoint (`docker/universal-inbox-entrypoint`) runs it after the database migrations and before starting the API server (`serve`) or the workers (`start-workers`), so nothing is served before the data is encrypted. On the first start after an upgrade, its duration depends on the amount of stored data. Several containers starting at once wait for each other.

The command then checks that every encrypted value can be read with the configured keys and stops the container otherwise: removing a key still in use cannot go unnoticed.

To see how many values each key encrypted:

```sh
universal-inbox data-encryption status
```

## Rotating the key

Rotate the key when it may have leaked, or periodically. The instance stays online during the whole procedure.

1. Generate a new key with `openssl rand -hex 32` and store it in your secret manager.
2. Add it with a new id and make it the active key, keeping the current key:

   ```sh
   UNIVERSAL_INBOX__DATA_ENCRYPTION__ACTIVE_KEY_ID=2
   UNIVERSAL_INBOX__DATA_ENCRYPTION__KEYS__1="<current key>"
   UNIVERSAL_INBOX__DATA_ENCRYPTION__KEYS__2="<new key>"
   ```

   If key `1` was only set as `UNIVERSAL_INBOX__OAUTH2__TOKEN_ENCRYPTION_KEY`, set `UNIVERSAL_INBOX__DATA_ENCRYPTION__KEYS__1` to its value.

3. Deploy the new configuration to the API server and the workers. New data is encrypted with key `2`, data encrypted with key `1` can still be read.
4. Re-encrypt the existing data with the new key, from a container with the new configuration:

   ```sh
   universal-inbox data-encryption reencrypt
   ```

   It works in batches while the instance is running. If it is interrupted, run it again: it resumes where it stopped.

5. Check that no value uses the old key anymore:

   ```sh
   universal-inbox data-encryption status
   ```

   Every line must only list `key 2` (and `plaintext: 0`). OAuth tokens never re-encrypted since the upgrade are listed as `key legacy`: they also use key `1` and are re-encrypted by step 4.

6. Remove `UNIVERSAL_INBOX__DATA_ENCRYPTION__KEYS__1` (and `UNIVERSAL_INBOX__OAUTH2__TOKEN_ENCRYPTION_KEY`, if it held the same key) and deploy. Containers refuse to start if some data still uses key `1`: run step 4 again in that case.
7. Keep the old key archived, outside the instance configuration, for as long as you keep database backups taken before step 4: restoring one of them requires it.

## Restoring a backup

A database backup can only be read with the keys that encrypted it. To restore a backup in another environment (for example to debug locally), configure the same keys there, including the archived keys of the backup's date.

## Removed plaintext

After the upgrade, the plaintext values are removed from the tables, but PostgreSQL keeps them in unused pages until they are overwritten. Run `VACUUM FULL` (or `pg_repack` to avoid locking the tables) on `third_party_item`, `task`, `user`, `user_auth`, `user_email_change`, `integration_connection`, `oauth_credential` and `oauth_grant_revocation` to remove them. Backups taken before the upgrade still contain plaintext data until they expire.
