//! Per-account login-attempt throttling, backed by Redis.
//!
//! This is the second layer of brute-force protection for the local-password
//! login path. The first layer (`utils::rate_limit`) caps total request volume
//! per IP; this layer caps *failed* attempts per account (keyed by email) and
//! temporarily locks the account with exponential backoff once a threshold is
//! crossed. Because the state lives in shared Redis (reusing the same
//! [`ConnectionManager`] as the cache and MCP session store), it survives API
//! restarts and is consistent across replicas — unlike the in-memory governor.
//!
//! **No account enumeration:** the counter is keyed by the submitted email
//! regardless of whether an account exists, and the caller returns an identical
//! throttled response for existing, non-existent, and locked accounts. The
//! email is SHA-256 hashed before use as a key so raw addresses never persist
//! in Redis.
//!
//! The mutate path (`record_failure`) runs as a single atomic Lua script
//! (`scripts/lua/login_throttle.lua`) so the increment / lock / TTL decision
//! cannot race across worker threads or pods.
//!
//! Tuning lives directly on [`LocalAuthenticationSettings`] (`max_login_attempts`,
//! `login_attempt_window_seconds`, `login_lockout_base_seconds`,
//! `login_lockout_max_seconds`).

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use email_address::EmailAddress;
use redis::{AsyncCommands, Script, aio::ConnectionManager};
use ring::digest;

use crate::{configuration::LocalAuthenticationSettings, universal_inbox::UniversalInboxError};

const NAMESPACE: &str = "universal-inbox:login-throttle:";

/// Outcome of recording a failed attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureOutcome {
    /// Post-increment consecutive-failure count for the account.
    pub fail_count: u32,
    /// Whether the account is locked as of this failure.
    pub locked: bool,
    /// Whether THIS failure started a fresh lock episode. The caller uses this
    /// to send the lockout-notification email exactly once per episode.
    pub newly_locked: bool,
    /// Seconds until the lock expires (0 when not locked). Surfaced as the
    /// `Retry-After` header.
    pub retry_after_seconds: u64,
}

/// Reference implementation of the backoff curve, kept in sync with
/// `scripts/lua/login_throttle.lua` (the Lua is authoritative at runtime; this
/// mirror exists so the curve can be unit-tested without a live Redis).
pub fn backoff_seconds(fail_count: u32, settings: &LocalAuthenticationSettings) -> u64 {
    if fail_count < settings.max_login_attempts {
        return 0;
    }
    let exponent = fail_count - settings.max_login_attempts;
    let factor = 2u64.saturating_pow(exponent);
    settings
        .login_lockout_base_seconds
        .saturating_mul(factor)
        .min(settings.login_lockout_max_seconds)
}

#[derive(Clone)]
pub struct LoginThrottle {
    conn: ConnectionManager,
    settings: LocalAuthenticationSettings,
}

impl LoginThrottle {
    pub fn new(conn: ConnectionManager, settings: LocalAuthenticationSettings) -> Self {
        Self { conn, settings }
    }

    /// SHA-256 of the lowercased address, namespaced. Emails are treated
    /// case-insensitively for throttling, and never stored in the clear.
    fn key(email: &EmailAddress) -> String {
        let normalized = email.to_string().to_lowercase();
        let hash = digest::digest(&digest::SHA256, normalized.as_bytes());
        format!("{NAMESPACE}{}", hex::encode(hash.as_ref()))
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Read-only check run at the start of a login attempt. Returns the number
    /// of seconds the account remains locked, or `None` if it may proceed.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn locked_for(
        &self,
        email: &EmailAddress,
    ) -> Result<Option<u64>, UniversalInboxError> {
        let mut conn = self.conn.clone();
        let key = Self::key(email);
        let locked_until: Option<i64> = conn
            .hget(&key, "locked_until")
            .await
            .context("Failed to read login throttle state from Redis")?;
        let now = Self::now_secs() as i64;
        match locked_until {
            Some(locked_until) if locked_until > now => Ok(Some((locked_until - now) as u64)),
            _ => Ok(None),
        }
    }

    /// Atomically record a failed attempt and report the resulting lock state.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn record_failure(
        &self,
        email: &EmailAddress,
    ) -> Result<FailureOutcome, UniversalInboxError> {
        let mut conn = self.conn.clone();
        let key = Self::key(email);
        let now = Self::now_secs();
        let (fail_count, locked_until, newly_locked): (i64, i64, i64) =
            Script::new(include_str!("../../scripts/lua/login_throttle.lua"))
                .key(&key)
                .arg(now)
                .arg(self.settings.max_login_attempts)
                .arg(self.settings.login_attempt_window_seconds)
                .arg(self.settings.login_lockout_base_seconds)
                .arg(self.settings.login_lockout_max_seconds)
                .invoke_async(&mut conn)
                .await
                .context("Failed to record failed login attempt in Redis")?;
        Ok(FailureOutcome {
            fail_count: fail_count.max(0) as u32,
            locked: locked_until > now as i64,
            newly_locked: newly_locked == 1,
            retry_after_seconds: (locked_until - now as i64).max(0) as u64,
        })
    }

    /// Clear the counter after a successful login.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn reset(&self, email: &EmailAddress) -> Result<(), UniversalInboxError> {
        let mut conn = self.conn.clone();
        let key = Self::key(email);
        let _: () = conn
            .del(&key)
            .await
            .context("Failed to reset login throttle state in Redis")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> LocalAuthenticationSettings {
        LocalAuthenticationSettings {
            argon2_algorithm: argon2::Algorithm::Argon2id,
            argon2_version: argon2::Version::V0x13,
            argon2_memory_size: 19456,
            argon2_iterations: 2,
            argon2_parallelism: 1,
            max_login_attempts: 5,
            login_attempt_window_seconds: 900,
            login_lockout_base_seconds: 60,
            login_lockout_max_seconds: 900,
        }
    }

    #[test]
    fn backoff_is_zero_below_threshold() {
        let settings = settings();
        for count in 0..settings.max_login_attempts {
            assert_eq!(backoff_seconds(count, &settings), 0);
        }
    }

    #[test]
    fn backoff_starts_at_base_and_doubles() {
        let settings = settings();
        assert_eq!(backoff_seconds(5, &settings), 60); // base
        assert_eq!(backoff_seconds(6, &settings), 120); // base * 2
        assert_eq!(backoff_seconds(7, &settings), 240); // base * 4
        assert_eq!(backoff_seconds(8, &settings), 480); // base * 8
    }

    #[test]
    fn backoff_is_capped_at_max() {
        let settings = settings();
        // base * 16 = 960 > 900 cap
        assert_eq!(backoff_seconds(9, &settings), 900);
        // far past the threshold stays capped (no overflow)
        assert_eq!(backoff_seconds(100, &settings), 900);
    }

    #[test]
    fn key_is_stable_and_case_insensitive() {
        let lower: EmailAddress = "user@example.com".parse().unwrap();
        let mixed: EmailAddress = "User@Example.com".parse().unwrap();
        assert_eq!(LoginThrottle::key(&lower), LoginThrottle::key(&mixed));
        assert!(LoginThrottle::key(&lower).starts_with(NAMESPACE));
        // SHA-256 hex is 64 chars; the raw email never appears in the key.
        assert_eq!(LoginThrottle::key(&lower).len(), NAMESPACE.len() + 64);
        assert!(!LoginThrottle::key(&lower).contains("user@example.com"));
    }
}
