use rstest::*;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

use universal_inbox::{
    auth::auth_token::AuthenticationToken,
    integration_connection::{
        IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{
            google_calendar::GoogleCalendarConfig, google_drive::GoogleDriveConfig,
            google_mail::GoogleMailConfig, ticktick::TickTickConfig,
        },
        provider::IntegrationProviderKind,
    },
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{create_integration_connection, get_integration_connection},
    tested_app,
};

fn no_redirect_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
}

/// Start a TickTick OAuth flow as the authenticated user and return the
/// `state` carried by the provider authorize URL.
async fn start_oauth_flow(app: &AuthenticatedApp) -> (IntegrationConnectionId, String) {
    start_oauth_flow_for(
        app,
        IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
        None,
    )
    .await
}

/// Start an OAuth flow for a new `Created` connection with `config`, pinned
/// to `provider_user_id` if given (a connection disconnected earlier keeps
/// its pinned provider account).
async fn start_oauth_flow_for(
    app: &AuthenticatedApp,
    config: IntegrationConnectionConfig,
    provider_user_id: Option<String>,
) -> (IntegrationConnectionId, String) {
    let integration_connection = create_integration_connection(
        &app.app,
        app.user.id,
        config,
        IntegrationConnectionStatus::Created,
        None,
        provider_user_id,
        None,
        None,
        None,
    )
    .await;

    let state = authorize_state(app, integration_connection.id).await;
    (integration_connection.id, state)
}

/// Start the OAuth authorization of `integration_connection_id` and return
/// the provider authorize URL.
async fn authorize_url(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
) -> url::Url {
    let body: Value = app
        .client
        .get(format!(
            "{}oauth/authorize-url/{}",
            app.app.api_address, integration_connection_id
        ))
        .send()
        .await
        .expect("Failed to start OAuth authorization")
        .json()
        .await
        .expect("Failed to parse authorize-url response");
    url::Url::parse(
        body["authorization_url"]
            .as_str()
            .expect("authorization_url"),
    )
    .expect("Invalid authorization URL")
}

/// Start the OAuth authorization of `integration_connection_id` and return
/// the `state` carried by the provider authorize URL.
async fn authorize_state(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
) -> String {
    authorize_url(app, integration_connection_id)
        .await
        .query_pairs()
        .find(|(key, _)| key == "state")
        .expect("Missing state in authorization URL")
        .1
        .to_string()
}

fn callback_location(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("expected Location header on redirect")
        .to_str()
        .expect("Location header must be ASCII")
        .to_string()
}

/// A callback carrying a state issued to another user must be refused before
/// the authorization code is exchanged: otherwise a victim following the
/// attacker's provider link would bind their provider account to the
/// attacker's connection.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_rejects_state_issued_to_another_user(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let (integration_connection_id, state) = start_oauth_flow(&app).await;

    // The victim is signed in to Universal Inbox with their own account.
    let (victim_client, _) =
        authenticate_user(&app.app, "5678", "Jane", "Roe", "jane@example.com").await;
    let victim_api_key: AuthenticationToken = victim_client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .header(
            reqwest::header::ORIGIN,
            crate::helpers::user::front_origin_header(&app.app),
        )
        .send()
        .await
        .expect("Failed to create API key")
        .json()
        .await
        .expect("Failed to deserialize API key response");

    let response = no_redirect_client_builder()
        .build()
        .unwrap()
        .get(format!(
            "{}/api/oauth/callback?state={}&code=victim-code",
            app.app.app_address.trim_end_matches('/'),
            urlencoding::encode(&state)
        ))
        .bearer_auth(&victim_api_key.jwt_token.expose_secret().0)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    let front_base_url = app.app.front_base_url.as_str().trim_end_matches('/');
    // `invalid-state`, not `provider-error`: the code was never exchanged.
    assert_eq!(
        callback_location(&response),
        format!("{front_base_url}/settings?oauth_error=invalid-state")
    );

    let integration_connection = get_integration_connection(&app, integration_connection_id)
        .await
        .expect("Integration connection must still exist");
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Created
    );
}

/// An anonymous callback (no Universal Inbox session) is refused without
/// consuming the state.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_requires_a_session(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let (_, state) = start_oauth_flow(&app).await;

    let response = no_redirect_client_builder()
        .build()
        .unwrap()
        .get(format!(
            "{}/api/oauth/callback?state={}&code=some-code",
            app.app.app_address.trim_end_matches('/'),
            urlencoding::encode(&state)
        ))
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    let front_base_url = app.app.front_base_url.as_str().trim_end_matches('/');
    assert_eq!(
        callback_location(&response),
        format!("{front_base_url}/settings?oauth_error=invalid-state")
    );

    let state_key = format!("universal-inbox::oauth-state::{state}");
    let mut conn = app.app.cache.connection_manager.clone();
    let still_stored: Option<String> = redis::AsyncCommands::get(&mut conn, &state_key)
        .await
        .expect("Failed to read OAuth state");
    assert!(
        still_stored.is_some(),
        "an anonymous callback must not consume the state"
    );
}

/// `/api/oauth/callback` used to inline the
/// `format!("{err}")` chain into the `oauth_error` query parameter of the
/// redirect, leaking internal context (Redis lookup failures, integration
/// connection IDs, provider error blobs) into the user-visible URL.
///
/// The fix maps internal errors to a small enum of public reason codes
/// (`invalid-state`, `expired-state`, `provider-error`, `internal-error`) and
/// logs the full chain server-side via `tracing::error!`. Only the code
/// reaches the URL.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_redacts_internal_error_chain(#[future] tested_app: TestedApp) {
    let app = tested_app.await;

    // Use a state value that does not exist in Redis. The service raises
    // `Unauthorized("Invalid or expired OAuth state")` for this case — our
    // classifier maps it to `invalid-state`, and crucially the original error
    // string must not appear in the redirect.
    let client = reqwest::Client::builder()
        // Capture the redirect ourselves instead of following it.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client");

    let url = format!(
        "{}/api/oauth/callback?state=does-not-exist&code=irrelevant",
        app.app_address
    );

    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(
        response.status(),
        reqwest::StatusCode::FOUND,
        "expected a 302 redirect"
    );

    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("expected Location header on redirect")
        .to_str()
        .expect("Location header must be ASCII");

    let front_base_url = app.front_base_url.as_str().trim_end_matches('/');
    let expected = format!("{front_base_url}/settings?oauth_error=invalid-state");
    assert_eq!(
        location, expected,
        "redirect Location must contain only the sanitized code, got: {location}"
    );

    // Defensive assertions: even if the redirect URL changes shape in the
    // future, none of the strings we used to leak should ever appear.
    assert!(
        !location.contains("Failed to retrieve"),
        "redirect leaks internal context: {location}"
    );
    assert!(
        !location.to_ascii_lowercase().contains("redis"),
        "redirect leaks Redis context: {location}"
    );
    assert!(
        !location.to_ascii_lowercase().contains("invalid or expired"),
        "redirect leaks raw error message: {location}"
    );
}

/// When the upstream OAuth provider returns its own error in the callback
/// query string (e.g. `error=access_denied`), we surface a generic
/// `provider-error` code instead of echoing the raw upstream value back to the
/// user. This prevents an attacker from crafting a callback URL that renders
/// arbitrary text in the user's URL bar / SPA toast.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_redacts_provider_error_parameter(#[future] tested_app: TestedApp) {
    let app = tested_app.await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client");

    // The previous implementation passed `error` straight through, so an
    // attacker-controlled value like `<script>` or a long inflammatory string
    // would land in the user's URL bar verbatim.
    let raw_provider_error = "access_denied: user clicked deny on consent screen";
    let url = format!(
        "{}/api/oauth/callback?error={}",
        app.app_address,
        urlencoding::encode(raw_provider_error)
    );

    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);

    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("expected Location header")
        .to_str()
        .expect("Location header must be ASCII");

    let front_base_url = app.front_base_url.as_str().trim_end_matches('/');
    assert_eq!(
        location,
        format!("{front_base_url}/settings?oauth_error=provider-error"),
    );
    assert!(
        !location.contains("access_denied"),
        "redirect echoes attacker-controlled provider error: {location}"
    );
    assert!(
        !location.contains("consent screen"),
        "redirect echoes raw upstream text: {location}"
    );
}

/// Missing `code` / `state` parameters indicate a malformed callback — surface
/// `invalid-state` instead of echoing a custom `missing_code` / `missing_state`
/// string that mixed casing styles and could grow over time.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_missing_state_returns_invalid_state(#[future] tested_app: TestedApp) {
    let app = tested_app.await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client");

    let url = format!("{}/api/oauth/callback?code=irrelevant", app.app_address);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("expected Location header")
        .to_str()
        .expect("Location header must be ASCII");

    let front_base_url = app.front_base_url.as_str().trim_end_matches('/');
    assert_eq!(
        location,
        format!("{front_base_url}/settings?oauth_error=invalid-state"),
    );
}

/// Deliver the provider callback with the connection owner's session.
async fn owner_callback(app: &AuthenticatedApp, state: &str) -> reqwest::Response {
    let api_key: AuthenticationToken = app
        .client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .header(
            reqwest::header::ORIGIN,
            crate::helpers::user::front_origin_header(&app.app),
        )
        .send()
        .await
        .expect("Failed to create API key")
        .json()
        .await
        .expect("Failed to deserialize API key response");

    no_redirect_client_builder()
        .build()
        .unwrap()
        .get(format!(
            "{}/api/oauth/callback?state={}&code=provider-code",
            app.app.app_address.trim_end_matches('/'),
            urlencoding::encode(state)
        ))
        .bearer_auth(&api_key.jwt_token.expose_secret().0)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request")
}

fn front_base_url(app: &AuthenticatedApp) -> String {
    app.app
        .front_base_url
        .as_str()
        .trim_end_matches('/')
        .to_string()
}

mod google_provider_user_id {
    use std::str::FromStr;

    use crate::helpers::integration_connection::google_account_id;

    use email_address::EmailAddress;
    use pretty_assertions::assert_eq;
    use universal_inbox::pii::Pii;

    use universal_inbox::{
        integration_connection::{
            integrations::google_mail::GoogleMailContext,
            provider::{IntegrationConnectionContext, IntegrationProvider},
        },
        notification::NotificationSourceKind,
    };
    use universal_inbox_api::{
        configuration::Settings,
        integrations::google_mail::{GoogleMailLabelList, GoogleMailThreadList},
    };

    use super::*;
    use crate::helpers::{
        notification::{
            google_mail::{
                google_mail_labels_list, mock_google_mail_labels_list_service,
                mock_google_mail_threads_list_service,
            },
            sync_notifications,
        },
        settings,
    };

    // The token the Google Mail sync mocks expect.
    const ACCESS_TOKEN: &str = "google_mail_test_access_token";

    fn google_config(provider_kind: IntegrationProviderKind) -> IntegrationConnectionConfig {
        match provider_kind {
            IntegrationProviderKind::GoogleMail => {
                IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled())
            }
            IntegrationProviderKind::GoogleCalendar => {
                IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled())
            }
            IntegrationProviderKind::GoogleDrive => {
                IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled())
            }
            _ => unreachable!("not a Google provider: {provider_kind}"),
        }
    }

    fn google_mock_server(
        app: &AuthenticatedApp,
        provider_kind: IntegrationProviderKind,
    ) -> &MockServer {
        match provider_kind {
            IntegrationProviderKind::GoogleMail => &app.app.google_mail_mock_server,
            IntegrationProviderKind::GoogleCalendar => &app.app.google_calendar_mock_server,
            IntegrationProviderKind::GoogleDrive => &app.app.google_drive_mock_server,
            _ => unreachable!("not a Google provider: {provider_kind}"),
        }
    }

    async fn mock_token_exchange(mock_server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": ACCESS_TOKEN,
                "refresh_token": "google-refresh-token",
                "token_type": "Bearer",
                "expires_in": 3599,
                "scope": "https://www.googleapis.com/auth/gmail.modify"
            })))
            .expect(1)
            .mount(mock_server)
            .await;
    }

    /// Mock the endpoint each Google integration reads the account email
    /// from, within the scopes it already requests.
    async fn mock_account_identity(
        mock_server: &MockServer,
        provider_kind: IntegrationProviderKind,
        email_address: &str,
    ) {
        let (mock_builder, body) = match provider_kind {
            IntegrationProviderKind::GoogleMail => (
                Mock::given(method("GET")).and(path("/users/me/profile")),
                json!({
                    "emailAddress": email_address,
                    "messagesTotal": 42,
                    "threadsTotal": 12,
                    "historyId": "1234"
                }),
            ),
            IntegrationProviderKind::GoogleCalendar => (
                Mock::given(method("GET")).and(path("/calendars/primary")),
                json!({
                    "kind": "calendar#calendar",
                    "id": email_address,
                    "summary": email_address,
                    "timeZone": "Europe/Paris"
                }),
            ),
            IntegrationProviderKind::GoogleDrive => (
                Mock::given(method("GET"))
                    .and(path("/about"))
                    .and(query_param("fields", "user(emailAddress)")),
                json!({ "user": { "emailAddress": email_address } }),
            ),
            _ => unreachable!("not a Google provider: {provider_kind}"),
        };
        mock_builder
            .and(header("authorization", format!("Bearer {ACCESS_TOKEN}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(mock_server)
            .await;
    }

    /// Google's token response does not identify the account, so the
    /// callback reads it from the integration's own API and pins it on the
    /// connection.
    #[rstest]
    #[case::google_mail(IntegrationProviderKind::GoogleMail)]
    #[case::google_calendar(IntegrationProviderKind::GoogleCalendar)]
    #[case::google_drive(IntegrationProviderKind::GoogleDrive)]
    #[tokio::test]
    async fn test_oauth_callback_pins_google_account(
        #[future] authenticated_app: AuthenticatedApp,
        #[case] provider_kind: IntegrationProviderKind,
    ) {
        let app = authenticated_app.await;
        let mock_server = google_mock_server(&app, provider_kind);
        mock_token_exchange(mock_server).await;
        mock_account_identity(mock_server, provider_kind, "Jane.Doe@example.com").await;
        let (integration_connection_id, state) =
            start_oauth_flow_for(&app, google_config(provider_kind), None).await;

        let response = owner_callback(&app, &state).await;

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert_eq!(
            callback_location(&response),
            format!("{}/settings?oauth_success=true", front_base_url(&app))
        );
        let integration_connection = get_integration_connection(&app, integration_connection_id)
            .await
            .expect("Integration connection must still exist");
        assert_eq!(
            integration_connection.status,
            IntegrationConnectionStatus::Validated
        );
        assert_eq!(
            integration_connection.provider_user_id,
            Some(google_account_id("jane.doe@example.com"))
        );
    }

    /// The callback reads the account email from `calendars.get` on the
    /// primary calendar, which `calendar.events` alone does not allow: the
    /// Google Calendar authorization must also ask for
    /// `calendar.calendars.readonly`.
    #[rstest]
    #[tokio::test]
    async fn test_google_calendar_authorize_url_requests_primary_calendar_read_scope(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app.app,
            app.user.id,
            google_config(IntegrationProviderKind::GoogleCalendar),
            IntegrationConnectionStatus::Created,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        let authorization_url = authorize_url(&app, integration_connection.id).await;

        let scope = authorization_url
            .query_pairs()
            .find(|(key, _)| key == "scope")
            .expect("Missing scope in authorization URL")
            .1
            .to_string();
        let scopes: Vec<&str> = scope.split(' ').collect();
        assert!(
            scopes.contains(&"https://www.googleapis.com/auth/calendar.calendars.readonly"),
            "Google Calendar authorization must request calendar.calendars.readonly, got {scopes:?}"
        );
        assert!(
            scopes.contains(&"https://www.googleapis.com/auth/calendar.events"),
            "Google Calendar authorization must request calendar.events, got {scopes:?}"
        );
    }

    /// Reconnecting a connection with the Google account it is pinned to
    /// validates it again.
    #[rstest]
    #[tokio::test]
    async fn test_oauth_callback_accepts_reconnect_with_pinned_google_account(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let provider_kind = IntegrationProviderKind::GoogleMail;
        let mock_server = google_mock_server(&app, provider_kind);
        mock_token_exchange(mock_server).await;
        mock_account_identity(mock_server, provider_kind, "jane.doe@example.com").await;
        let (integration_connection_id, state) = start_oauth_flow_for(
            &app,
            google_config(provider_kind),
            Some(google_account_id("jane.doe@example.com")),
        )
        .await;

        let response = owner_callback(&app, &state).await;

        assert_eq!(
            callback_location(&response),
            format!("{}/settings?oauth_success=true", front_base_url(&app))
        );
        let integration_connection = get_integration_connection(&app, integration_connection_id)
            .await
            .expect("Integration connection must still exist");
        assert_eq!(
            integration_connection.status,
            IntegrationConnectionStatus::Validated
        );
        assert_eq!(
            integration_connection.provider_user_id,
            Some(google_account_id("jane.doe@example.com"))
        );
    }

    /// Reconnecting with another Google account than the pinned one is
    /// allowed: the connection is re-pinned to the new account, and the Gmail
    /// user address cached from the previous account is refreshed on the next
    /// sync.
    #[rstest]
    #[tokio::test]
    async fn test_oauth_callback_repins_reconnect_with_another_google_account(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_labels_list: GoogleMailLabelList,
    ) {
        let app = authenticated_app.await;
        let mock_server = &app.app.google_mail_mock_server;
        mock_token_exchange(mock_server).await;
        // Read once by the callback, then once by the sync to refresh the
        // cached address, keeping Gmail's spelling.
        Mock::given(method("GET"))
            .and(path("/users/me/profile"))
            .and(header("authorization", format!("Bearer {ACCESS_TOKEN}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "emailAddress": "John.Roe@example.com",
                "messagesTotal": 42,
                "threadsTotal": 12,
                "historyId": "1234"
            })))
            .expect(2)
            .mount(mock_server)
            .await;
        let google_mail_config = GoogleMailConfig::enabled();
        let integration_connection = create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            IntegrationConnectionStatus::Created,
            Some(IntegrationConnectionContext::GoogleMail(
                GoogleMailContext {
                    user_email_address: Pii::<EmailAddress>::from_str("jane.doe@example.com")
                        .unwrap(),
                    labels: vec![],
                },
            )),
            Some(google_account_id("jane.doe@example.com")),
            None,
            None,
            None,
        )
        .await;
        let state = authorize_state(&app, integration_connection.id).await;

        let response = owner_callback(&app, &state).await;

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert_eq!(
            callback_location(&response),
            format!("{}/settings?oauth_success=true", front_base_url(&app))
        );
        let reconnected_integration_connection =
            get_integration_connection(&app, integration_connection.id)
                .await
                .expect("Integration connection must still exist");
        assert_eq!(
            reconnected_integration_connection.status,
            IntegrationConnectionStatus::Validated
        );
        assert_eq!(
            reconnected_integration_connection.provider_user_id,
            Some(google_account_id("john.roe@example.com"))
        );

        mock_google_mail_labels_list_service(mock_server, &google_mail_labels_list).await;
        mock_google_mail_threads_list_service(
            mock_server,
            None,
            settings
                .integrations
                .get("google_mail")
                .unwrap()
                .page_size
                .unwrap(),
            Some(vec![google_mail_config.synced_label.id.clone()]),
            &GoogleMailThreadList {
                threads: None,
                result_size_estimate: 0,
                next_page_token: None,
            },
        )
        .await;
        sync_notifications(
            &app.client,
            &app.app.api_address,
            Some(NotificationSourceKind::GoogleMail),
            false,
        )
        .await;

        let synced_integration_connection =
            get_integration_connection(&app, integration_connection.id)
                .await
                .expect("Integration connection must still exist");
        let IntegrationProvider::GoogleMail {
            context: Some(GoogleMailContext {
                user_email_address, ..
            }),
            ..
        } = synced_integration_connection.provider
        else {
            panic!("Google Mail integration connection must have a context");
        };
        assert_eq!(
            user_email_address,
            Pii::<EmailAddress>::from_str("John.Roe@example.com").unwrap()
        );
    }

    /// When the Google account cannot be read, the callback fails closed:
    /// the connection is neither validated nor pinned.
    #[rstest]
    #[tokio::test]
    async fn test_oauth_callback_fails_when_google_account_cannot_be_read(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let provider_kind = IntegrationProviderKind::GoogleDrive;
        let mock_server = google_mock_server(&app, provider_kind);
        mock_token_exchange(mock_server).await;
        Mock::given(method("GET"))
            .and(path("/about"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(mock_server)
            .await;
        let (integration_connection_id, state) =
            start_oauth_flow_for(&app, google_config(provider_kind), None).await;

        let response = owner_callback(&app, &state).await;

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert_eq!(
            callback_location(&response),
            format!(
                "{}/settings?oauth_error=internal-error",
                front_base_url(&app)
            )
        );
        let integration_connection = get_integration_connection(&app, integration_connection_id)
            .await
            .expect("Integration connection must still exist");
        assert_eq!(
            integration_connection.status,
            IntegrationConnectionStatus::Created
        );
        assert_eq!(integration_connection.provider_user_id, None);
    }
}

mod notion_provider {
    use std::str::FromStr;

    use base64::{Engine, engine::general_purpose::STANDARD};
    use email_address::EmailAddress;
    use pretty_assertions::assert_eq;
    use universal_inbox::{
        integration_connection::{
            integrations::notion::{NotionConfig, NotionContext},
            provider::IntegrationProvider,
        },
        pii::Pii,
    };
    use universal_inbox_api::configuration::Settings;
    use wiremock::matchers::body_string_contains;

    use super::*;
    use crate::helpers::{
        notion::{
            BLOCK_ID, DISCUSSION_ID, PAGE_ID, notion_comment_event, post_signed_notion_event,
            pushed_jobs_count,
        },
        settings,
    };

    /// Mock Notion's token exchange, installed by `notion-user-id` in the
    /// workspace `notion-workspace-id`.
    async fn mock_notion_token_exchange(app: &AuthenticatedApp, settings: &Settings) {
        let notion_settings = &settings.integrations["notion"];
        Mock::given(method("POST"))
            .and(path("/v1/oauth/token"))
            .and(header(
                "authorization",
                format!(
                    "Basic {}",
                    STANDARD.encode(format!(
                        "{}:{}",
                        notion_settings.oauth_client_id,
                        notion_settings.oauth_client_secret.as_str()
                    ))
                ),
            ))
            .and(body_string_contains("grant_type=authorization_code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "notion-access-token",
                "token_type": "bearer",
                "bot_id": "notion-bot-id",
                "workspace_id": "notion-workspace-id",
                "workspace_name": "Acme",
                "workspace_icon": null,
                "owner": {
                    "type": "user",
                    "user": {
                        "object": "user",
                        "id": "notion-user-id",
                        "name": "Jane Doe",
                        "avatar_url": null,
                        "type": "person",
                        "person": { "email": "jane.doe@example.com" }
                    }
                },
                "duplicated_template_id": null
            })))
            .expect(1)
            .mount(&app.app.notion_mock_server)
            .await;
    }

    /// An event of a workspace nobody had connected yet is dropped, and the
    /// answer is cached: connecting the workspace must clear it, so its next
    /// event is queued.
    #[rstest]
    #[tokio::test]
    async fn test_notion_events_are_queued_once_the_workspace_is_connected(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        mock_notion_token_exchange(&app, &settings).await;
        let event = notion_comment_event(
            "notion-workspace-id",
            "comment.created",
            "comment-1",
            "notion-user-someone",
            DISCUSSION_ID,
            PAGE_ID,
            BLOCK_ID,
            &["notion-user-id"],
        );
        post_signed_notion_event(&app.client, &app.app.api_address, &event).await;
        assert_eq!(pushed_jobs_count(&app.app).await, 0);

        let (_, state) = start_oauth_flow_for(
            &app,
            IntegrationConnectionConfig::Notion(NotionConfig::default()),
            None,
        )
        .await;
        let response = owner_callback(&app, &state).await;
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        post_signed_notion_event(&app.client, &app.app.api_address, &event).await;

        assert_eq!(pushed_jobs_count(&app.app).await, 1);
    }

    /// Notion's token response names the human who installed the integration
    /// (`owner.user`) and the workspace: the callback pins that user and keeps
    /// the workspace and bot ids the webhooks are routed with.
    #[rstest]
    #[tokio::test]
    async fn test_oauth_callback_pins_notion_owner_and_stores_context(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        mock_notion_token_exchange(&app, &settings).await;
        let (integration_connection_id, state) = start_oauth_flow_for(
            &app,
            IntegrationConnectionConfig::Notion(NotionConfig::default()),
            None,
        )
        .await;

        let response = owner_callback(&app, &state).await;

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert_eq!(
            callback_location(&response),
            format!("{}/settings?oauth_success=true", front_base_url(&app))
        );
        let integration_connection = get_integration_connection(&app, integration_connection_id)
            .await
            .expect("Integration connection must still exist");
        assert_eq!(
            integration_connection.status,
            IntegrationConnectionStatus::Validated
        );
        assert_eq!(
            integration_connection.provider_user_id,
            Some("notion-user-id".to_string())
        );
        assert_eq!(
            integration_connection.provider,
            IntegrationProvider::Notion {
                context: Some(NotionContext {
                    workspace_id: "notion-workspace-id".to_string(),
                    workspace_name: Some("Acme".to_string()),
                    bot_id: "notion-bot-id".to_string(),
                    user_display_name: Some(Pii::new("Jane Doe".to_string())),
                    user_email_address: Some(Pii::new(
                        EmailAddress::from_str("jane.doe@example.com").unwrap()
                    )),
                    comments_reconciled_at: None,
                }),
                config: NotionConfig {
                    sync_comments_enabled: true,
                    sync_mentions_enabled: true,
                },
            }
        );
    }

    /// Notion asks for `owner=user` and grants access per page, not per scope.
    #[rstest]
    #[tokio::test]
    async fn test_notion_authorize_url_asks_for_a_user_owner(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Notion(NotionConfig::default()),
            IntegrationConnectionStatus::Created,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        let body: Value = app
            .client
            .get(format!(
                "{}oauth/authorize-url/{}",
                app.app.api_address, integration_connection.id
            ))
            .send()
            .await
            .expect("Failed to start OAuth authorization")
            .json()
            .await
            .expect("Failed to parse authorize-url response");
        let authorization_url =
            url::Url::parse(body["authorization_url"].as_str().unwrap()).unwrap();

        assert_eq!(
            format!(
                "{}://{}{}",
                authorization_url.scheme(),
                authorization_url.host_str().unwrap(),
                authorization_url.path()
            ),
            "https://api.notion.com/v1/oauth/authorize"
        );
        let params: Vec<(String, String)> = authorization_url
            .query_pairs()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        assert!(params.contains(&("owner".to_string(), "user".to_string())));
        assert!(params.contains(&(
            "client_id".to_string(),
            settings.integrations["notion"].oauth_client_id.clone()
        )));
        assert!(!params.iter().any(|(key, _)| key == "scope"));
    }
}
