//! DB state mutations used to reproduce error/edge-state UIs against the
//! throwaway screenshot user. Each helper opens its own transaction, mutates,
//! and commits. Failures are mapped to `anyhow::Error` so the orchestrator
//! can log a per-spec failure without aborting the whole run.

use std::sync::Arc;

use anyhow::Context;
use chrono::Utc;
use tokio::sync::RwLock;
use tracing::info;
use universal_inbox::{
    integration_connection::{IntegrationConnectionStatus, provider::IntegrationProviderKind},
    user::UserId,
};
use uuid::Uuid;

use crate::universal_inbox::{
    integration_connection::service::IntegrationConnectionService, user::service::UserService,
};

async fn find_connection_id(
    service: &IntegrationConnectionService,
    user_id: UserId,
    kind: IntegrationProviderKind,
) -> anyhow::Result<universal_inbox::integration_connection::IntegrationConnectionId> {
    let mut tx = service
        .begin()
        .await
        .context("Failed to begin transaction while resolving integration connection")?;
    let connections = service
        .fetch_all_integration_connections(&mut tx, user_id, None, false)
        .await
        .with_context(|| format!("Failed to fetch integration connections for user {user_id}"))?;
    tx.rollback().await.ok();
    connections
        .into_iter()
        .find(|c| c.provider.kind() == kind)
        .map(|c| c.id)
        .ok_or_else(|| {
            anyhow::anyhow!("No {kind} integration connection seeded for user {user_id}")
        })
}

/// Sets the GitHub integration connection to `Created` (Disconnected) status —
/// the seeded OAuth credentials are removed and the status leaf flips to
/// "disconnected" on the settings page.
pub async fn set_github_disconnected(
    service: Arc<RwLock<IntegrationConnectionService>>,
    user_id: UserId,
) -> anyhow::Result<()> {
    let svc = service.read().await;
    let id = find_connection_id(&svc, user_id, IntegrationProviderKind::Github).await?;
    let mut tx = svc
        .begin()
        .await
        .context("Failed to begin transaction while disconnecting GitHub")?;
    svc.disconnect_integration_connection(&mut tx, id, user_id)
        .await
        .context("Failed to disconnect GitHub integration connection")?;
    tx.commit()
        .await
        .context("Failed to commit GitHub disconnect transaction")?;
    info!("GitHub integration connection {id} forced to Disconnected (Created)");
    Ok(())
}

/// Keeps the GitHub connection Validated but clears `registered_oauth_scopes`,
/// which forces the UI's "missing required scopes" banner.
pub async fn set_github_missing_scopes(
    service: Arc<RwLock<IntegrationConnectionService>>,
    user_id: UserId,
) -> anyhow::Result<()> {
    let svc = service.read().await;
    let id = find_connection_id(&svc, user_id, IntegrationProviderKind::Github).await?;
    let mut tx = svc
        .begin()
        .await
        .context("Failed to begin transaction while clearing GitHub OAuth scopes")?;
    svc.force_set_integration_connection_state(
        &mut tx,
        id,
        IntegrationConnectionStatus::Validated,
        None,
        Some(Vec::new()),
        user_id,
    )
    .await
    .context("Failed to force GitHub missing-scopes state")?;
    tx.commit()
        .await
        .context("Failed to commit missing-scopes transaction")?;
    info!("GitHub integration connection {id} forced to Validated with empty OAuth scopes");
    Ok(())
}

/// Seeds two fake API tokens and one authorized OAuth2 client for the test
/// user so the /security page has content to render for the AI-agents and
/// API-usage doc screenshots.
///
/// Uses raw SQL so we do not have to thread `auth_token_service` and
/// `oauth2_service` into the screenshot orchestrator. The inserted tokens use
/// non-JWT placeholder strings — the screenshots tool only navigates the UI;
/// no code path actually validates these tokens.
pub async fn seed_security_artifacts(
    user_service: Arc<UserService>,
    user_id: UserId,
) -> anyhow::Result<()> {
    let mut tx = user_service
        .begin()
        .await
        .context("Failed to begin transaction while seeding security artifacts")?;
    let now = Utc::now();
    let expire_at = now + chrono::Duration::days(90);

    for label in ["doc-screenshot-key-1", "doc-screenshot-key-2"] {
        // jwt_token is UNIQUE and NOT NULL — concatenating the label with a
        // fresh UUID keeps the constraint happy without producing a real JWT.
        let token = format!("ui-doc-{}-{}", label, Uuid::new_v4());
        sqlx::query(
            r#"
                INSERT INTO authentication_token
                  (id, user_id, jwt_token, expire_at, is_revoked, is_session_token, created_at, updated_at)
                VALUES ($1, $2, $3, $4::timestamp, false, false, $5::timestamp, $5::timestamp)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(user_id.0)
        .bind(&token)
        .bind(expire_at.naive_utc())
        .bind(now.naive_utc())
        .execute(&mut *tx)
        .await
        .with_context(|| format!("Failed to insert seed authentication_token {label}"))?;
    }

    // Authorized OAuth2 client + matching refresh token.
    let client_id = format!("doc-screenshot-client-{}", Uuid::new_v4());
    sqlx::query(
        r#"
            INSERT INTO oauth2_client
              (client_id, client_name, redirect_uris, grant_types, response_types, token_endpoint_auth_method)
            VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(&client_id)
    .bind("Claude")
    .bind(vec!["https://example.com/callback".to_string()])
    .bind(vec!["authorization_code".to_string(), "refresh_token".to_string()])
    .bind(vec!["code".to_string()])
    .bind("none")
    .execute(&mut *tx)
    .await
    .context("Failed to insert seed oauth2_client")?;

    sqlx::query(
        r#"
            INSERT INTO oauth2_refresh_token
              (id, token_hash, client_id, user_id, scope, expires_at, created_at, revoked_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(format!("doc-screenshot-hash-{}", Uuid::new_v4()))
    .bind(&client_id)
    .bind(user_id.0)
    .bind("mcp")
    .bind(expire_at)
    .bind(now)
    .execute(&mut *tx)
    .await
    .context("Failed to insert seed oauth2_refresh_token")?;

    tx.commit()
        .await
        .context("Failed to commit security-artifacts transaction")?;
    info!("Seeded 2 API keys + 1 authorized OAuth client for user {user_id}");
    Ok(())
}
