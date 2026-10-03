//! Per-user revocation of browser sessions, backed by Redis.
//!
//! Session JWTs live in a signed cookie and are not stored server-side, so they
//! cannot be revoked one by one. Instead, changing or resetting a password
//! records a per-user cutoff: session JWTs issued before it are rejected by the
//! authentication middleware (see `middlewares::jwt_auth::SessionTokenChecker`).
//!
//! The cutoff only needs to outlive the sessions it revokes, so its key expires
//! with the session JWT lifetime. It is stored in seconds, like the JWT `iat`
//! claim: a session issued in the same second as the cutoff stays valid, which
//! is what lets the user who changed their password keep their own session.

use anyhow::Context;
use chrono::{DateTime, Utc};
use redis::{AsyncCommands, aio::ConnectionManager};
use universal_inbox::user::UserId;

use crate::universal_inbox::UniversalInboxError;

const NAMESPACE: &str = "universal-inbox:sessions-revoked-before:";

pub struct SessionRevocation {
    conn: ConnectionManager,
    ttl_seconds: u64,
}

impl SessionRevocation {
    pub fn new(conn: ConnectionManager, ttl_seconds: u64) -> Self {
        Self { conn, ttl_seconds }
    }

    fn key(user_id: UserId) -> String {
        format!("{NAMESPACE}{user_id}")
    }

    /// Revoke every session of `user_id` issued before `revoked_before`.
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn revoke_sessions(
        &self,
        user_id: UserId,
        revoked_before: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .set_ex(
                Self::key(user_id),
                revoked_before.timestamp(),
                self.ttl_seconds,
            )
            .await
            .context("Failed to store session revocation in Redis")?;
        Ok(())
    }

    /// Whether a session of `user_id` issued at `issued_at` (unix seconds) has
    /// not been revoked.
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn is_session_active(
        &self,
        user_id: UserId,
        issued_at: i64,
    ) -> Result<bool, UniversalInboxError> {
        let mut conn = self.conn.clone();
        let revoked_before: Option<i64> = conn
            .get(Self::key(user_id))
            .await
            .context("Failed to read session revocation from Redis")?;
        Ok(revoked_before.is_none_or(|revoked_before| issued_at >= revoked_before))
    }
}
