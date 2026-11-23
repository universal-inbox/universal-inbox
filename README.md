# Universal Inbox

[![Apache 2 License](https://img.shields.io/badge/license-Apache%202-blue.svg)](https://www.apache.org/licenses/)
[![Coverage Status](https://coveralls.io/repos/github/universal-inbox/universal-inbox/badge.svg?branch=main)](https://coveralls.io/github/universal-inbox/universal-inbox?branch=main)
[![CI](https://github.com/universal-inbox/universal-inbox/workflows/CI/badge.svg)](https://github.com/universal-inbox/universal-inbox/actions)

Universal Inbox is a solution that centralizes all your notifications and tasks in one place to create a unique inbox.

## Features

- Synchronize notifications from:
  - Github
  - Linear
  - Google Mail
  - ... (more to come)
- Synchronize tasks from:
  - Todoist
- Act on notifications:
  - delete the notification and a new one will be received if the underlying resource (issue, pull request, project, ...) is updated
  - unsubscribe the notification, it is deleted and no new one will be received unless a new mention appears in the underlying resource
  - snooze the notification to make it disappear until the next day
  - create and plan a task in the connected task service (Todoist for now)
  - associate the notification to an existing task
- 2 ways synchronization: Universal Inbox tries as much as possible to keep its notifications state in sync with the upstream service. The upstream service API does not always permit it.

## Development

### Pre-requisites

The development environment is using [mise](https://mise.jdx.dev/) to install the tools, set the environment variables and run the services.
Before setting up the Universal Inbox environment, you have to [install mise](https://mise.jdx.dev/getting-started.html) and [activate it in your shell](https://mise.jdx.dev/getting-started.html#activate-mise).

mise cannot provide the system libraries used to build the Rust dependencies:

- Linux: a C toolchain, `pkg-config` and the OpenSSL headers (`build-essential pkg-config libssl-dev` on Debian and Ubuntu)
- macOS: the Xcode command line tools (`xcode-select --install`), which provide `iconv` and the Foundation framework

### Environment setup

```bash
git clone https://github.com/universal-inbox/universal-inbox.git
cd universal-inbox
mise trust
mise install
```

`mise install` installs the tools declared in `mise.toml` (Rust, Node.js, `just`, `sqlx-cli`, `dioxus-cli`, Playwright Chromium, ...). Once mise is activated in your shell, entering the directory loads the environment variables of the `[env]` section, the `.env` file and the git-ignored `mise.local.toml`.

#### Setup PostgreSQL

Start PostgreSQL and Redis (their data is kept outside of the repository by mise, see `mise daemons ls --json`):

```bash
just run-detached
```

`just run` does the same and opens the services dashboard. Other services (`ui-api`, `ui-workers`, `ui-web`, ...) are started with `just start <service>`, stopped with `just stop <service>`, and their output is shown with `just logs <service>`. `just status` shows the state of the services and `just down` stops them all.

Prepare the database:

```bash
just migrate-db
```

### Build the application

```bash
just build-all
```

### Execute tests

Before executing the tests, Postgres and Redis must be running:

```bash
just test-all
```

### OpenIDConnect service

Universal Inbox uses OpenIDConnect to implement the user authentication and it relies on a third party OIDC service.
Thus it must be configured to use this OIDC service using the following configuration variables, using environment variables:

```
AUTHENTICATION_OIDC_ISSUER_URL=https://oidc.service
AUTHENTICATION_OIDC_INTROSPECTION_URL=https://oidc.service/oauth/v2/introspect
AUTHENTICATION_OIDC_FRONT_CLIENT_ID=1234@universal_inbox
AUTHENTICATION_OIDC_API_CLIENT_ID=1234@universal_inbox
AUTHENTICATION_OIDC_API_CLIENT_SECRET=secret
AUTHENTICATION_USER_PROFILE_URL=https://oidc.service/users/me
```

Or using a `api/config/local.toml` file:

```toml
[application.authentication]
oidc_issuer_url = "https://service"
oidc_introspection_url = "https://service/oauth/v2/introspect"
oidc_front_client_id = "1234@universal_inbox"
oidc_api_client_id = "1234@universal_inbox"
oidc_api_client_secret = "secret"
user_profile_url = "https://oidc.service/users/me"
```

### Start the application

```bash
just run
```

It will start the following services:

- `postgresql` to store Universal Inbox data
- `redis` to store the HTTP sessions
- `ui-api` is the Universal Inbox rest API
- `ui-web` is the Universal Inbox frontend

You can the connect the development application on [http://localhost:8080](http://localhost:8080).

### Configure third-party OAuth applications

Universal Inbox handles the OAuth2 authorization flow natively. For each integration you want to enable (GitHub, Linear, Slack, Notion, Google Mail/Calendar/Drive, Todoist, TickTick), declare Universal Inbox as an OAuth2 application with the provider and copy the resulting `oauth_client_id` / `oauth_client_secret` into the matching `[integrations.<name>]` section in `api/config/local.toml` (or via `UNIVERSAL_INBOX__INTEGRATIONS__<NAME>__OAUTH_CLIENT_ID` environment variables). The OAuth redirect URI registered with each provider must be `{front_base_url}/api/oauth/callback`.

#### Notion

1. Create a **public** integration at <https://www.notion.so/profile/integrations> with the redirect URI `{front_base_url}/api/oauth/callback` and these capabilities: *Read content*, *Read comments* and *Read user information including email addresses*. Set its client ID and secret as `UNIVERSAL_INBOX__INTEGRATIONS__NOTION__OAUTH_CLIENT_ID` / `UNIVERSAL_INBOX__INTEGRATIONS__NOTION__OAUTH_CLIENT_SECRET`.
2. In the integration's **Webhooks** tab, create a subscription to `{front_base_url}/api/hooks/notion/events` with the events `comment.created`, `comment.updated`, `comment.deleted` and `page.content_updated`. The URL must be reachable from the internet.
3. Notion sends a verification token, and the API logs the first one it receives after it starts. Paste it in Notion's **Verify** dialog first: Notion accepts only the token it sent, so an accepted token is genuine (if Verify fails, restart the API and click **Resend token**). Then set it as `UNIVERSAL_INBOX__INTEGRATIONS__NOTION__SIGNING_SECRET` and restart the API. Until it is set, every Notion event is rejected. This token is the signing key of every event: treat the logs that hold it as secret.

Capabilities are fixed when a workspace installs the integration: after changing them, remove the integration from the workspace connections and connect again.

## License

[Apache 2 License](LICENSE)
