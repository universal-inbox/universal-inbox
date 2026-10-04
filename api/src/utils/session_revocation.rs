//! Revocation of browser sessions, backed by Redis.
//!
//! Session JWTs live in a signed cookie and are not stored server-side. Two kinds
//! of revocation are recorded instead, both checked by the authentication
//! middleware (see `middlewares::jwt_auth::SessionTokenChecker`):
//!
//! - per session: logging out records the JWT `jti`, so that session stops
//!   authenticating even if its cookie was copied;
//! - per user: changing or resetting a password records a cutoff, and session
//!   JWTs issued before it are rejected.
//!
//! Each entry only needs to outlive the sessions it revokes, so its key expires
//! with the session JWT lifetime. The cutoff is stored in seconds, like the JWT
//! `iat` claim: a session issued in the same second as the cutoff stays valid,
//! which is what lets the user who changed their password keep their own session.

use anyhow::Context;
use chrono::{DateTime, Utc};
use redis::{AsyncCommands, aio::ConnectionManager};
use universal_inbox::user::UserId;

use crate::{observability::attr, universal_inbox::UniversalInboxError};

const USER_NAMESPACE: &str = "universal-inbox:sessions-revoked-before:";
const SESSION_NAMESPACE: &str = "universal-inbox:session-revoked:";

pub struct SessionRevocation {
    conn: ConnectionManager,
    ttl_seconds: u64,
}

impl SessionRevocation {
    pub fn new(conn: ConnectionManager, ttl_seconds: u64) -> Self {
        Self { conn, ttl_seconds }
    }

    fn user_key(user_id: UserId) -> String {
        format!("{USER_NAMESPACE}{user_id}")
    }

    fn session_key(token_id: &str) -> String {
        format!("{SESSION_NAMESPACE}{token_id}")
    }

    /// Revoke every session of `user_id` issued before `revoked_before`.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::USER_ID } = user_id.to_string(),
            { attr::USER_SESSIONS_REVOKED_BEFORE } = revoked_before.to_rfc3339()
        )
    )]
    pub async fn revoke_sessions(
        &self,
        user_id: UserId,
        revoked_before: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .set_ex(
                Self::user_key(user_id),
                revoked_before.timestamp(),
                self.ttl_seconds,
            )
            .await
            .context("Failed to store session revocation in Redis")?;
        Ok(())
    }

    /// Revoke the single session whose JWT has the `token_id` (`jti`) claim and
    /// expires at `expires_at` (unix seconds). The entry expires with the JWT.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::AUTH_JWT_EXPIRES_AT } = expires_at)
    )]
    pub async fn revoke_session(
        &self,
        token_id: &str,
        expires_at: i64,
    ) -> Result<(), UniversalInboxError> {
        let ttl_seconds = (expires_at - Utc::now().timestamp()).max(1) as u64;
        let mut conn = self.conn.clone();
        let _: () = conn
            .set_ex(Self::session_key(token_id), 1, ttl_seconds)
            .await
            .context("Failed to store session revocation in Redis")?;
        Ok(())
    }

    /// Whether the session of `user_id` with JWT `token_id` (`jti`), issued at
    /// `issued_at` (unix seconds), has been revoked neither by itself nor by a
    /// user-wide cutoff.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::USER_ID } = user_id.to_string(),
            { attr::AUTH_JWT_ISSUED_AT } = issued_at
        )
    )]
    pub async fn is_session_active(
        &self,
        user_id: UserId,
        token_id: &str,
        issued_at: i64,
    ) -> Result<bool, UniversalInboxError> {
        let mut conn = self.conn.clone();
        let (revoked_before, session_revoked): (Option<i64>, Option<i64>) = conn
            .mget(&[Self::user_key(user_id), Self::session_key(token_id)])
            .await
            .context("Failed to read session revocation from Redis")?;
        Ok(session_revoked.is_none()
            && revoked_before.is_none_or(|revoked_before| issued_at >= revoked_before))
    }
}
