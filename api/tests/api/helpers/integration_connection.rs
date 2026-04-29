use chrono::{DateTime, Utc};
use reqwest::{Client, Response};
use rstest::fixture;
use slack_morphism::SlackTeamId;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::slack::SlackContext,
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
    },
    user::UserId,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::oauth2::{AccessToken, RefreshToken},
    repository::{
        integration_connection::{
            IntegrationConnectionRepository, IntegrationConnectionSyncedBeforeFilter,
        },
        oauth_credential::OAuthCredentialRepository,
    },
    universal_inbox::UpdateStatus,
    utils::crypto::{TokenEncryptionKey, encrypt_token},
};

use crate::helpers::{TestedApp, auth::AuthenticatedApp};

/// Lightweight test fixture describing the OAuth credential state of an
/// integration connection. Replaces the legacy Nango-shaped JSON fixtures and
/// the bespoke `NangoConnection` struct. Tests build one of these (typically
/// via the per-provider fixture functions below) and pass it to
/// [`create_and_mock_integration_connection`], which persists the data the
/// same way the runtime OAuth callback would.
#[derive(Debug, Clone)]
pub struct OAuthCredentialFixture {
    pub access_token: AccessToken,
    pub refresh_token: Option<RefreshToken>,
    pub provider_user_id: Option<String>,
    pub registered_oauth_scopes: Vec<String>,
}

pub async fn list_integration_connections_response(client: &Client, api_address: &str) -> Response {
    client
        .get(format!("{api_address}integration-connections"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_integration_connections(
    client: &Client,
    api_address: &str,
) -> Vec<IntegrationConnection> {
    list_integration_connections_response(client, api_address)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

#[allow(clippy::too_many_arguments)]
pub async fn create_integration_connection(
    app: &TestedApp,
    user_id: UserId,
    config: IntegrationConnectionConfig,
    status: IntegrationConnectionStatus,
    context: Option<IntegrationConnectionContext>,
    provider_user_id: Option<String>,
    initial_sync_failures: Option<u32>,
    first_notifications_sync_failed_at: Option<DateTime<Utc>>,
    registered_oauth_scopes: Option<Vec<String>>,
) -> Box<IntegrationConnection> {
    let mut transaction = app.repository.begin().await.unwrap();
    let mut new_integration_connection =
        IntegrationConnection::new(user_id, config, IntegrationConnectionStatus::Created);
    if let Some(initial_sync_failures) = initial_sync_failures {
        new_integration_connection.notifications_sync_failures = initial_sync_failures;
    }
    if let Some(first_failed_at) = first_notifications_sync_failed_at {
        new_integration_connection.first_notifications_sync_failed_at = Some(first_failed_at);
    }
    let integration_connection = app
        .repository
        .create_integration_connection(&mut transaction, Box::new(new_integration_connection))
        .await
        .unwrap();

    if let Some(provider_user_id) = provider_user_id {
        app.repository
            .update_integration_connection_provider_user_id(
                &mut transaction,
                integration_connection.id,
                Some(provider_user_id),
            )
            .await
            .unwrap();
    }

    let _update_result = app
        .repository
        .update_integration_connection_status(
            &mut transaction,
            integration_connection.id,
            status,
            None,
            registered_oauth_scopes,
            user_id,
        )
        .await
        .unwrap();

    let update_result = app
        .repository
        .update_integration_connection_context(&mut transaction, integration_connection.id, context)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    update_result.result.unwrap()
}

pub async fn get_integration_connection_per_provider(
    app: &AuthenticatedApp,
    user_id: UserId,
    provider_kind: IntegrationProviderKind,
    synced_before_filter: Option<IntegrationConnectionSyncedBeforeFilter>,
    with_status: Option<IntegrationConnectionStatus>,
) -> Option<IntegrationConnection> {
    let mut transaction = app.app.repository.begin().await.unwrap();
    let integration_connection = app
        .app
        .repository
        .get_integration_connection_per_provider(
            &mut transaction,
            user_id,
            provider_kind,
            synced_before_filter,
            with_status,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    integration_connection
}

pub async fn get_integration_connection(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
) -> Option<IntegrationConnection> {
    let mut transaction = app.app.repository.begin().await.unwrap();
    let integration_connection = app
        .app
        .repository
        .get_integration_connection(&mut transaction, integration_connection_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    integration_connection
}

pub async fn update_integration_connection_context(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
    context: IntegrationConnectionContext,
) -> UpdateStatus<Box<IntegrationConnection>> {
    let mut transaction = app.app.repository.begin().await.unwrap();
    let result = app
        .app
        .repository
        .update_integration_connection_context(
            &mut transaction,
            integration_connection_id,
            Some(context),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    result
}

/// Create an integration connection in `Validated` state and persist the access
/// token from the given fixture as an encrypted `oauth_credential` row.
/// Mirrors what the production OAuth callback does after a successful exchange.
pub async fn create_and_mock_integration_connection(
    app: &TestedApp,
    user_id: UserId,
    config: IntegrationConnectionConfig,
    settings: &Settings,
    credential: OAuthCredentialFixture,
    initial_sync_failures: Option<u32>,
    context: Option<IntegrationConnectionContext>,
) -> Box<IntegrationConnection> {
    create_and_mock_integration_connection_with_backoff(
        app,
        user_id,
        config,
        settings,
        credential,
        initial_sync_failures,
        None,
        context,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_and_mock_integration_connection_with_backoff(
    app: &TestedApp,
    user_id: UserId,
    config: IntegrationConnectionConfig,
    settings: &Settings,
    credential: OAuthCredentialFixture,
    initial_sync_failures: Option<u32>,
    first_notifications_sync_failed_at: Option<DateTime<Utc>>,
    context: Option<IntegrationConnectionContext>,
) -> Box<IntegrationConnection> {
    let integration_connection = create_integration_connection(
        app,
        user_id,
        config,
        IntegrationConnectionStatus::Validated,
        context,
        credential.provider_user_id.clone(),
        initial_sync_failures,
        first_notifications_sync_failed_at,
        Some(credential.registered_oauth_scopes.clone()),
    )
    .await;

    let token_encryption_key =
        TokenEncryptionKey::from_hex(&settings.oauth2.token_encryption_key).unwrap();
    let aad_context = integration_connection.id.0.as_bytes();
    let encrypted_access_token = encrypt_token(
        credential.access_token.as_str(),
        aad_context,
        &token_encryption_key,
    )
    .unwrap();
    let encrypted_refresh_token = credential
        .refresh_token
        .as_ref()
        .map(|rt| encrypt_token(rt.as_str(), aad_context, &token_encryption_key).unwrap());

    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .store_oauth_credential(
            &mut transaction,
            integration_connection.id,
            encrypted_access_token,
            encrypted_refresh_token,
            // Tests don't exercise the eager-refresh path, so we leave the
            // expiry unset and the runtime read path will skip the refresh
            // check.
            None,
            serde_json::json!({}),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    integration_connection
}

// --- Per-provider fixture builders. Each returns a fresh, mutable
// `OAuthCredentialFixture` so individual tests can tweak it (e.g., bumping
// `provider_user_id` to simulate a different Slack workspace member) before
// passing it to `create_and_mock_integration_connection`.

#[fixture]
pub fn github_oauth_credential() -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken("github_test_access_token".to_string()),
        refresh_token: Some(RefreshToken("github_test_refresh_token".to_string())),
        provider_user_id: None,
        registered_oauth_scopes: vec![
            "notifications".to_string(),
            "read:discussion".to_string(),
            "read:org".to_string(),
            "repo".to_string(),
        ],
    }
}

#[fixture]
pub fn linear_oauth_credential() -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken("linear_test_access_token".to_string()),
        refresh_token: Some(RefreshToken("linear_test_refresh_token".to_string())),
        provider_user_id: None,
        registered_oauth_scopes: vec!["read".to_string(), "write".to_string()],
    }
}

#[fixture]
pub fn google_calendar_oauth_credential() -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken("google_calendar_test_access_token".to_string()),
        refresh_token: Some(RefreshToken(
            "google_calendar_test_refresh_token".to_string(),
        )),
        provider_user_id: None,
        registered_oauth_scopes: vec!["https://www.googleapis.com/auth/calendar".to_string()],
    }
}

#[fixture]
pub fn google_mail_oauth_credential() -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken("google_mail_test_access_token".to_string()),
        refresh_token: Some(RefreshToken("google_mail_test_refresh_token".to_string())),
        provider_user_id: None,
        registered_oauth_scopes: vec!["https://www.googleapis.com/auth/gmail.modify".to_string()],
    }
}

#[fixture]
pub fn google_drive_oauth_credential() -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken("google_drive_test_access_token".to_string()),
        refresh_token: Some(RefreshToken("google_drive_test_refresh_token".to_string())),
        provider_user_id: None,
        registered_oauth_scopes: vec![
            "https://www.googleapis.com/auth/drive.readonly".to_string(),
            "https://www.googleapis.com/auth/drive.metadata.readonly".to_string(),
        ],
    }
}

#[fixture]
pub fn slack_oauth_credential() -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken("slack_test_user_access_token".to_string()),
        refresh_token: None,
        provider_user_id: Some("U05XXX".to_string()),
        registered_oauth_scopes: vec![
            "channels:history".to_string(),
            "channels:read".to_string(),
            "emoji:read".to_string(),
            "groups:history".to_string(),
            "groups:read".to_string(),
            "im:history".to_string(),
            "im:read".to_string(),
            "mpim:history".to_string(),
            "mpim:read".to_string(),
            "reactions:read".to_string(),
            "reactions:write".to_string(),
            "team:read".to_string(),
            "usergroups:read".to_string(),
            "users:read".to_string(),
        ],
    }
}

/// Convenience helper: a Slack `IntegrationConnectionContext` carrying a team id.
pub fn slack_context(team_id: impl Into<String>) -> IntegrationConnectionContext {
    IntegrationConnectionContext::Slack(SlackContext {
        team_id: SlackTeamId(team_id.into()),
        extension_credentials: vec![],
        last_extension_heartbeat_at: None,
    })
}

#[fixture]
pub fn todoist_oauth_credential() -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken("todoist_test_access_token".to_string()),
        refresh_token: None,
        provider_user_id: None,
        registered_oauth_scopes: vec![],
    }
}

#[fixture]
pub fn ticktick_oauth_credential() -> OAuthCredentialFixture {
    OAuthCredentialFixture {
        access_token: AccessToken("ticktick_test_access_token".to_string()),
        refresh_token: None,
        provider_user_id: None,
        registered_oauth_scopes: vec!["tasks:read".to_string(), "tasks:write".to_string()],
    }
}

pub async fn create_ticktick_integration_connection(
    app: &TestedApp,
    user_id: UserId,
    settings: &Settings,
    config: IntegrationConnectionConfig,
    initial_sync_failures: Option<u32>,
) -> Box<IntegrationConnection> {
    create_and_mock_integration_connection(
        app,
        user_id,
        config,
        settings,
        ticktick_oauth_credential(),
        initial_sync_failures,
        None,
    )
    .await
}
