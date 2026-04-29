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

The development environment is using [Devbox](https://www.jetpack.io/devbox/) which is based on Nix.
Before setting up the Universal Inbox environment, you have to install Devbox following these [instructions](https://www.jetpack.io/devbox/docs/quickstart/#install-devbox).

### Environment setup

```bash
git clone https://github.com/universal-inbox/universal-inbox.git
```

The simplest is to install [direnv](https://direnv.net/) to enter a complete development environment everytime you enter the `universal-inbox` directory:

```bash
cd universal-inbox
direnv allow
```

From here, it should keep the environment installed using Devbox.

#### Environment setup (bis)

Without direnv, you can start by installing the development environment:

```bash
cd universal-inbox
devbox install
```

and then enter the environment:

```bash
devbox shell
```

#### Setup PostgreSQL

Start PostgreSQL (it will also start Redis):

```bash
just run
```

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

Universal Inbox handles the OAuth2 authorization flow natively. For each integration you want to enable (GitHub, Linear, Slack, Google Mail/Calendar/Drive, Todoist, TickTick), declare Universal Inbox as an OAuth2 application with the provider and copy the resulting `oauth_client_id` / `oauth_client_secret` into the matching `[integrations.<name>]` section in `api/config/local.toml` (or via `UNIVERSAL_INBOX__INTEGRATIONS__<NAME>__OAUTH_CLIENT_ID` environment variables). The OAuth redirect URI registered with each provider must be `{front_base_url}/api/oauth/callback`.

## License

[Apache 2 License](LICENSE)
