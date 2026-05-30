use std::sync::Arc;

use anyhow::Context;
use base64::prelude::*;
use chrono::{TimeDelta, Utc};
use jsonwebtoken::{EncodingKey, Header};
use rand::RngExt;
use ring::digest;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::{
    auth::oauth2::{AuthorizedOAuth2Client, OAuth2Client, OAuth2UserConsent, TokenResponse},
    user::UserId,
};

use crate::{
    configuration::CimdSettings,
    repository::{Repository, oauth2::OAuth2Repository},
    universal_inbox::{
        UniversalInboxError,
        oauth2::cimd::{self, is_cimd_client_id},
    },
    utils::jwt::{Claims, JWT_SIGNING_ALGO, JWTBase64EncodedSigningKeys, JWTSigningKeys},
};

const ACCESS_TOKEN_EXPIRY_SECS: u64 = 3600;
const REFRESH_TOKEN_EXPIRY_DAYS: i64 = 30;
const AUTH_CODE_EXPIRY_SECS: i64 = 60;

pub struct OAuth2Service {
    repository: Arc<Repository>,
    jwt_encoding_key: EncodingKey,
    resource_url: String,
    cimd_settings: CimdSettings,
}

impl OAuth2Service {
    pub fn new(
        repository: Arc<Repository>,
        jwt_secret_key: String,
        jwt_public_key: String,
        resource_url: String,
        cimd_settings: CimdSettings,
    ) -> Self {
        let jwt_signing_keys =
            JWTSigningKeys::load_from_base64_encoded_keys(JWTBase64EncodedSigningKeys {
                secret_key: jwt_secret_key,
                public_key: jwt_public_key,
            })
            .expect("Failed to load JWT signing keys for OAuth2 service");
        Self {
            repository,
            jwt_encoding_key: jwt_signing_keys.encoding_key.clone(),
            resource_url,
            cimd_settings,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn register_client(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        client_name: Option<String>,
        redirect_uris: Vec<String>,
    ) -> Result<OAuth2Client, UniversalInboxError> {
        if redirect_uris.is_empty() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "redirect_uris must contain at least one URI".to_string(),
            });
        }
        for uri in &redirect_uris {
            validate_redirect_uri(uri)?;
        }

        let client_id = Uuid::new_v4().to_string();
        self.repository
            .create_oauth2_client(
                transaction,
                &client_id,
                client_name.as_deref(),
                &redirect_uris,
            )
            .await
    }

    #[tracing::instrument(level = "debug", skip_all, fields(client_id), err)]
    pub async fn get_client(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        client_id: &str,
    ) -> Result<Option<OAuth2Client>, UniversalInboxError> {
        self.repository
            .get_oauth2_client_by_client_id(transaction, client_id)
            .await
    }

    /// Resolve a `client_id` to an [`OAuth2Client`], regardless of whether it
    /// was created via RFC 7591 dynamic client registration (opaque UUID-shaped
    /// `client_id`) or Client ID Metadata Discovery (`https://...` URL
    /// `client_id`).
    ///
    /// For CIMD clients, the metadata document is fetched lazily and cached
    /// in `oauth2_client_metadata_cache`. Subsequent calls within the cache
    /// TTL avoid the network round-trip entirely. The resulting `OAuth2Client`
    /// is synthesised from the document — there is no row in `oauth2_client`
    /// for CIMD clients.
    #[tracing::instrument(level = "debug", skip_all, fields(client_id), err)]
    pub async fn resolve_client(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        client_id: &str,
    ) -> Result<Option<OAuth2Client>, UniversalInboxError> {
        if is_cimd_client_id(client_id) {
            self.resolve_cimd_client(transaction, client_id)
                .await
                .map(Some)
        } else {
            self.get_client(transaction, client_id).await
        }
    }

    /// Internal: load (or refetch) a CIMD client by metadata URL and project
    /// the validated document into the shared [`OAuth2Client`] shape.
    ///
    /// On every resolve we also UPSERT into `oauth2_client` so that the
    /// existing FK constraints on `oauth2_authorization_code` /
    /// `oauth2_refresh_token` / `oauth2_user_consent` continue to point at a
    /// real row — the CIMD doc is the source of truth for `redirect_uris`
    /// and `client_name`, so we mirror it on every refresh.
    async fn resolve_cimd_client(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        client_id_url: &str,
    ) -> Result<OAuth2Client, UniversalInboxError> {
        let cached = self
            .repository
            .get_cimd_metadata_cache(transaction, client_id_url)
            .await?;
        let now = Utc::now();
        let document = if let Some(row) = cached
            && row.expires_at > now
        {
            row.document
        } else {
            let fetched = cimd::fetch_and_validate(client_id_url, &self.cimd_settings).await?;
            let new_expires_at =
                now + TimeDelta::from_std(fetched.ttl).unwrap_or_else(|_| TimeDelta::seconds(3600));
            self.repository
                .upsert_cimd_metadata_cache(
                    transaction,
                    client_id_url,
                    &fetched.document,
                    &fetched.body_sha256,
                    new_expires_at,
                )
                .await?;
            fetched.document
        };

        // FK shadow row — see [`OAuth2Repository::upsert_cimd_oauth2_client`].
        let client = self
            .repository
            .upsert_cimd_oauth2_client(
                transaction,
                client_id_url,
                document.client_name.as_deref(),
                &document.redirect_uris,
            )
            .await?;

        Ok(client)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    #[allow(clippy::too_many_arguments)]
    pub async fn create_authorization_code(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        client_id: &str,
        user_id: UserId,
        redirect_uri: &str,
        scope: Option<&str>,
        code_challenge: &str,
        code_challenge_method: &str,
        resource: Option<&str>,
    ) -> Result<String, UniversalInboxError> {
        if code_challenge_method != "S256" {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "Only S256 code_challenge_method is supported".to_string(),
            });
        }

        // Verify the client exists (DCR row OR CIMD metadata document) and
        // the redirect_uri is registered.
        let client = self
            .resolve_client(transaction, client_id)
            .await?
            .ok_or_else(|| UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Unknown client_id: {client_id}"),
            })?;

        if !redirect_uri_matches_registered(&client.redirect_uris, redirect_uri) {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Invalid redirect_uri: {redirect_uri}"),
            });
        }

        let code = generate_random_token();

        let expires_at = Utc::now()
            + TimeDelta::try_seconds(AUTH_CODE_EXPIRY_SECS).unwrap_or_else(|| {
                panic!("Invalid AUTH_CODE_EXPIRY_SECS value: {AUTH_CODE_EXPIRY_SECS}")
            });

        self.repository
            .create_authorization_code(
                transaction,
                &code,
                client_id,
                user_id,
                redirect_uri,
                scope,
                code_challenge,
                resource,
                expires_at,
            )
            .await?;

        Ok(code)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(client_id), err)]
    pub async fn exchange_code(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> Result<TokenResponse, UniversalInboxError> {
        let auth_code = self
            .repository
            .get_and_delete_authorization_code(transaction, code)
            .await?
            .ok_or_else(|| UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "Invalid or expired authorization code".to_string(),
            })?;

        // Verify the code has not expired
        if auth_code.expires_at < Utc::now() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "Authorization code has expired".to_string(),
            });
        }

        // Verify client_id matches
        if auth_code.client_id != client_id {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "client_id mismatch".to_string(),
            });
        }

        // Verify redirect_uri matches
        if auth_code.redirect_uri != redirect_uri {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "redirect_uri mismatch".to_string(),
            });
        }

        // Verify PKCE: SHA-256(code_verifier) == code_challenge
        verify_pkce(code_verifier, &auth_code.code_challenge)?;

        let scope = auth_code.scope.clone().unwrap_or_default();
        let resource = auth_code.resource.as_deref().unwrap_or(&self.resource_url);

        // Generate access token (JWT)
        let access_token =
            self.create_access_token(auth_code.user_id, &scope, client_id, resource)?;

        // Generate refresh token
        let refresh_token_raw = generate_random_token();
        let refresh_token_hash = hash_token(&refresh_token_raw);

        let refresh_expires_at = Utc::now()
            + TimeDelta::try_days(REFRESH_TOKEN_EXPIRY_DAYS).unwrap_or_else(|| {
                panic!("Invalid REFRESH_TOKEN_EXPIRY_DAYS value: {REFRESH_TOKEN_EXPIRY_DAYS}")
            });

        self.repository
            .create_refresh_token(
                transaction,
                &refresh_token_hash,
                client_id,
                auth_code.user_id,
                auth_code.scope.as_deref(),
                auth_code.resource.as_deref(),
                Some(refresh_expires_at),
            )
            .await?;

        Ok(TokenResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: ACCESS_TOKEN_EXPIRY_SECS,
            refresh_token: refresh_token_raw,
            scope,
        })
    }

    /// Rotate a refresh token, atomically.
    ///
    /// **Security:** the previous implementation did
    /// `SELECT by hash → expiry check → UPDATE by hash` as three separate
    /// statements. Two concurrent `/token` requests with the same refresh
    /// token could both pass the SELECT before either revoke fired, then both
    /// mint fresh `(access, refresh)` pairs — effectively duplicating a
    /// session from a single leaked credential until expiry. RFC 6749 §10.4
    /// and RFC 6819 §5.2.2.3 require treating refresh-token reuse as a
    /// compromise signal and revoking the entire token family.
    ///
    /// The fix collapses claim+revoke into a single
    /// `UPDATE … WHERE token_hash = $1 AND client_id = $2 AND revoked_at IS
    /// NULL AND (expires_at IS NULL OR expires_at > now()) RETURNING …`. Only
    /// one caller can win that race; the loser sees `None` and falls into
    /// reuse-detection: if a row with that hash exists but is already
    /// revoked, every refresh token for `(client_id, user_id)` is revoked,
    /// including any token freshly minted by the legitimate winning branch.
    /// The expiry check is folded into the same SQL so an expired row is
    /// never claimed.
    #[tracing::instrument(level = "debug", skip_all, fields(client_id), err)]
    pub async fn refresh_token(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        refresh_token: &str,
        client_id: &str,
    ) -> Result<TokenResponse, UniversalInboxError> {
        let token_hash = hash_token(refresh_token);

        // Atomically claim-and-revoke. `Some(row)` ⇒ this caller won the
        // race; `None` ⇒ already-revoked / never-existed / expired / wrong
        // client. We MUST NOT mint new tokens in the `None` branch.
        let stored_token = match self
            .repository
            .revoke_refresh_token_if_active(transaction, &token_hash, client_id)
            .await?
        {
            Some(row) => row,
            None => {
                // Reuse detection: re-read by hash alone (ignoring
                // `revoked_at`). If the row exists, the caller presented a
                // token that DID match a known credential but has already
                // been consumed (or belongs to a different client_id) — a
                // strong reuse signal. Revoke the entire family for that
                // `(client_id, user_id)` per RFC 6819 §5.2.2.3.
                if let Some(existing) = self
                    .repository
                    .get_refresh_token_by_hash_including_revoked(transaction, &token_hash)
                    .await?
                {
                    // The caller's transaction will be rolled back by the
                    // route handler's `?`-propagation (returning Err from
                    // this function aborts the surrounding tx). Open a fresh
                    // transaction and commit it *before* returning the error,
                    // so the family revocation actually persists even on the
                    // failure path.
                    //
                    // Revoke the family of the original `client_id` recorded
                    // on the row — not the caller-supplied `client_id`. A
                    // client-id-mismatch attacker shouldn't be able to direct
                    // a family revocation at an unrelated client. (We still
                    // revoke; we just revoke the right family.)
                    let mut revoke_tx = self.repository.begin().await?;
                    let revoked_count = self
                        .repository
                        .revoke_all_refresh_tokens_for_client(
                            &mut revoke_tx,
                            existing.user_id,
                            &existing.client_id,
                        )
                        .await?;
                    revoke_tx
                        .commit()
                        .await
                        .map_err(|err| UniversalInboxError::DatabaseError {
                            message: format!(
                                "Failed to commit refresh-token family revocation: {err}"
                            ),
                            source: err,
                        })?;
                    tracing::warn!(
                        user_id = %existing.user_id,
                        client_id = %existing.client_id,
                        revoked_count,
                        "Refresh token reuse detected — revoked entire token family"
                    );
                }
                return Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "Invalid refresh token".to_string(),
                });
            }
        };

        let scope = stored_token.scope.clone().unwrap_or_default();
        let resource = stored_token
            .resource
            .as_deref()
            .unwrap_or(&self.resource_url);

        // Generate new access token
        let access_token =
            self.create_access_token(stored_token.user_id, &scope, client_id, resource)?;

        // Generate new refresh token
        let new_refresh_token_raw = generate_random_token();
        let new_refresh_token_hash = hash_token(&new_refresh_token_raw);

        let refresh_expires_at = Utc::now()
            + TimeDelta::try_days(REFRESH_TOKEN_EXPIRY_DAYS).unwrap_or_else(|| {
                panic!("Invalid REFRESH_TOKEN_EXPIRY_DAYS value: {REFRESH_TOKEN_EXPIRY_DAYS}")
            });

        self.repository
            .create_refresh_token(
                transaction,
                &new_refresh_token_hash,
                client_id,
                stored_token.user_id,
                stored_token.scope.as_deref(),
                stored_token.resource.as_deref(),
                Some(refresh_expires_at),
            )
            .await?;

        Ok(TokenResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: ACCESS_TOKEN_EXPIRY_SECS,
            refresh_token: new_refresh_token_raw,
            scope,
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    pub async fn list_authorized_clients(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<AuthorizedOAuth2Client>, UniversalInboxError> {
        self.repository
            .list_authorized_clients(transaction, user_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    pub async fn revoke_client_authorization(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
    ) -> Result<u64, UniversalInboxError> {
        // Drop the stored consent so the user is re-prompted on the next
        // authorize request, even before any refresh token has been issued.
        self.repository
            .delete_user_consent(transaction, user_id, client_id)
            .await?;
        self.repository
            .revoke_all_refresh_tokens_for_client(transaction, user_id, client_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    pub async fn get_user_consent(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
    ) -> Result<Option<OAuth2UserConsent>, UniversalInboxError> {
        self.repository
            .get_user_consent(transaction, user_id, client_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    pub async fn record_user_consent(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
        scope: &str,
    ) -> Result<OAuth2UserConsent, UniversalInboxError> {
        self.repository
            .upsert_user_consent(transaction, user_id, client_id, scope)
            .await
    }

    /// Returns true when `stored_scope` covers every space-separated token in
    /// `requested_scope` (treating both as sets). An empty requested scope is
    /// always considered covered.
    pub fn consent_covers_scope(stored_scope: &str, requested_scope: Option<&str>) -> bool {
        let requested = requested_scope.unwrap_or("");
        let stored_set: std::collections::HashSet<&str> = stored_scope.split_whitespace().collect();
        requested
            .split_whitespace()
            .all(|token| stored_set.contains(token))
    }

    fn create_access_token(
        &self,
        user_id: UserId,
        scope: &str,
        client_id: &str,
        resource: &str,
    ) -> Result<String, UniversalInboxError> {
        let now = Utc::now();
        let expires_at = now
            + TimeDelta::try_seconds(ACCESS_TOKEN_EXPIRY_SECS as i64).unwrap_or_else(|| {
                panic!("Invalid ACCESS_TOKEN_EXPIRY_SECS value: {ACCESS_TOKEN_EXPIRY_SECS}")
            });

        let claims = Claims {
            iat: now.timestamp() as usize,
            exp: expires_at.timestamp() as usize,
            sub: user_id.to_string(),
            jti: Uuid::new_v4().to_string(),
            aud: Some(resource.to_string()),
            scope: Some(scope.to_string()),
            client_id: Some(client_id.to_string()),
        };

        jsonwebtoken::encode(
            &Header::new(JWT_SIGNING_ALGO),
            &claims,
            &self.jwt_encoding_key,
        )
        .context("Failed to encode OAuth2 access token")
        .map_err(Into::into)
    }
}

fn generate_random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    BASE64_URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> String {
    let digest = digest::digest(&digest::SHA256, token.as_bytes());
    BASE64_URL_SAFE_NO_PAD.encode(digest.as_ref())
}

fn verify_pkce(code_verifier: &str, code_challenge: &str) -> Result<(), UniversalInboxError> {
    let computed_challenge = hash_token(code_verifier);
    if computed_challenge != code_challenge {
        return Err(UniversalInboxError::InvalidInputData {
            source: None,
            user_error: "PKCE verification failed".to_string(),
        });
    }
    Ok(())
}

/// Validate that a redirect_uri is safe to register: must be `https://` (any
/// host) or `http://` restricted to loopback hosts. Schemes like `javascript:`,
/// `data:`, `file:`, etc. are rejected outright.
pub fn validate_redirect_uri(uri: &str) -> Result<(), UniversalInboxError> {
    let parsed = url::Url::parse(uri).map_err(|err| UniversalInboxError::InvalidInputData {
        source: None,
        user_error: format!("Invalid redirect_uri {uri}: {err}"),
    })?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" => {
            if is_loopback_host(parsed.host_str()) {
                Ok(())
            } else {
                Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: format!("http redirect_uri must be loopback: {uri}"),
                })
            }
        }
        other => Err(UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("Unsupported redirect_uri scheme '{other}': {uri}"),
        }),
    }
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(
        host,
        Some("localhost") | Some("127.0.0.1") | Some("[::1]") | Some("::1")
    )
}

/// Returns true when `uri` parses as an `http://` URL with a loopback host
/// (`localhost`, `127.0.0.1`, `::1`). Used to gate unauthenticated dynamic
/// client registration to the MCP-spec public DCR path for desktop/CLI clients.
pub fn is_loopback_redirect_uri(uri: &str) -> bool {
    url::Url::parse(uri)
        .map(|parsed| parsed.scheme() == "http" && is_loopback_host(parsed.host_str()))
        .unwrap_or(false)
}

/// Returns true when `redirect_uri`'s origin (scheme + host + port,
/// normalized per RFC 6454) appears in the configured allow-list. Each entry
/// of `allowed_origins` is expected to be a full origin string (e.g.
/// `https://claude.ai`); we parse both sides as URLs and compare
/// [`url::Url::origin`] rather than naive string match so that
/// `https://claude.ai/api/mcp/auth_callback` matches `https://claude.ai`
/// even though the strings differ.
///
/// Plain `http://` redirect_uris are never accepted by this check — only the
/// existing [`is_loopback_redirect_uri`] check accepts those. This keeps the
/// host allow-list a strict superset of the loopback path while never
/// trusting plain HTTP to a third-party host.
pub fn is_origin_in_allowlist(redirect_uri: &str, allowed_origins: &[String]) -> bool {
    let Ok(parsed) = url::Url::parse(redirect_uri) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    let uri_origin = parsed.origin();
    allowed_origins.iter().any(|origin_str| {
        url::Url::parse(origin_str)
            .map(|origin_url| origin_url.origin() == uri_origin)
            .unwrap_or(false)
    })
}

/// Returns true when `requested` should be accepted as the redirect_uri for a
/// client whose registered redirect_uris are `registered`.
///
/// Non-loopback URIs require an exact string match. Loopback URIs
/// (`http://localhost`, `http://127.0.0.1`, `http://[::1]`) match any
/// registered loopback URI that agrees on scheme, host, path, and query while
/// **ignoring the port** — as required by RFC 8252 §7.3, which mandates that
/// the authorization server "allow any port to be specified at the time of the
/// request for loopback IP redirect URIs", so native/CLI apps (e.g. the MCP
/// SDK in Claude Code) can bind an OS-assigned ephemeral port per request while
/// reusing a persisted dynamically-registered client.
///
/// The relaxation is scoped strictly to loopback hosts: the requested host must
/// itself be loopback, and only registered entries with the *same* loopback
/// host are considered (so `localhost` never matches `127.0.0.1`). Everything
/// else still falls through to exact matching.
pub fn redirect_uri_matches_registered(registered: &[String], requested: &str) -> bool {
    if registered.iter().any(|uri| uri == requested) {
        return true;
    }

    let Ok(requested_url) = url::Url::parse(requested) else {
        return false;
    };
    if requested_url.scheme() != "http" || !is_loopback_host(requested_url.host_str()) {
        return false;
    }

    registered.iter().any(|candidate| {
        let Ok(candidate_url) = url::Url::parse(candidate) else {
            return false;
        };
        candidate_url.scheme() == "http"
            && candidate_url.host_str() == requested_url.host_str()
            && candidate_url.path() == requested_url.path()
            && candidate_url.query() == requested_url.query()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression tests for `validate_redirect_uri`, pinning the scheme
    /// allow-list introduced in 88b30871 (security fix for
    /// universal-inbox-bkj.25). Only `https://...` and `http://` against
    /// loopback hosts (`localhost`, `127.0.0.1`, `[::1]`) are accepted;
    /// everything else — including `javascript:`, `data:`, `file:`, custom
    /// app schemes like `intent:`, and unparseable strings — must be
    /// rejected with `InvalidInputData`.
    fn assert_rejected(uri: &str) {
        let result = validate_redirect_uri(uri);
        assert!(
            matches!(result, Err(UniversalInboxError::InvalidInputData { .. })),
            "expected {uri:?} to be rejected with InvalidInputData, got {result:?}"
        );
    }

    fn assert_accepted(uri: &str) {
        let result = validate_redirect_uri(uri);
        assert!(
            result.is_ok(),
            "expected {uri:?} to be accepted, got {result:?}"
        );
    }

    #[test]
    fn accepts_https_uri() {
        assert_accepted("https://example.com/cb");
    }

    #[test]
    fn accepts_https_uri_with_fragment() {
        // The current implementation does not reject fragments. This test
        // pins the existing behavior so a future tightening is an explicit
        // decision rather than a silent regression. (RFC 6749 §3.1.2 forbids
        // fragments in redirect_uri, but enforcing that is a separate change.)
        assert_accepted("https://example.com/cb#fragment");
    }

    #[test]
    fn accepts_http_loopback_localhost() {
        assert_accepted("http://localhost:8080/cb");
    }

    #[test]
    fn accepts_http_loopback_ipv4() {
        assert_accepted("http://127.0.0.1/cb");
    }

    #[test]
    fn accepts_http_loopback_ipv6() {
        assert_accepted("http://[::1]/cb");
    }

    #[test]
    fn rejects_http_non_loopback() {
        assert_rejected("http://example.com/cb");
    }

    #[test]
    fn rejects_javascript_scheme() {
        assert_rejected("javascript:alert(1)");
    }

    #[test]
    fn rejects_data_scheme() {
        assert_rejected("data:text/html,<script>alert(1)</script>");
    }

    #[test]
    fn rejects_file_scheme() {
        assert_rejected("file:///etc/passwd");
    }

    #[test]
    fn rejects_intent_scheme() {
        assert_rejected("intent://scan/#Intent;scheme=https;package=com.evil");
    }

    #[test]
    fn rejects_unparseable_uri() {
        assert_rejected("not a url at all");
    }

    /// Tests for `redirect_uri_matches_registered`, pinning the RFC 8252 §7.3
    /// loopback port-agnostic relaxation (universal-inbox-2q1). The AS must
    /// accept any port on a loopback redirect so the MCP SDK can bind an
    /// ephemeral OS port per request while reusing a persisted DCR client;
    /// non-loopback URIs must still match exactly.
    fn reg(uris: &[&str]) -> Vec<String> {
        uris.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn exact_match_still_accepted() {
        assert!(redirect_uri_matches_registered(
            &reg(&["http://localhost:8080/callback"]),
            "http://localhost:8080/callback"
        ));
        assert!(redirect_uri_matches_registered(
            &reg(&["https://claude.ai/api/mcp/auth_callback"]),
            "https://claude.ai/api/mcp/auth_callback"
        ));
    }

    #[test]
    fn loopback_localhost_matches_any_port() {
        // Registered with one port, requested with another (the bug scenario).
        assert!(redirect_uri_matches_registered(
            &reg(&["http://localhost:8080/callback"]),
            "http://localhost:56010/callback"
        ));
    }

    #[test]
    fn loopback_ipv4_and_ipv6_match_any_port() {
        assert!(redirect_uri_matches_registered(
            &reg(&["http://127.0.0.1:8080/callback"]),
            "http://127.0.0.1:49152/callback"
        ));
        assert!(redirect_uri_matches_registered(
            &reg(&["http://[::1]:8080/callback"]),
            "http://[::1]:49152/callback"
        ));
    }

    #[test]
    fn loopback_different_path_rejected() {
        assert!(!redirect_uri_matches_registered(
            &reg(&["http://localhost:8080/callback"]),
            "http://localhost:56010/evil"
        ));
    }

    #[test]
    fn loopback_different_query_rejected() {
        assert!(!redirect_uri_matches_registered(
            &reg(&["http://localhost:8080/callback?a=1"]),
            "http://localhost:56010/callback?a=2"
        ));
    }

    #[test]
    fn loopback_host_must_match_localhost_not_ip() {
        // localhost and 127.0.0.1 are distinct hosts; the relaxation only
        // ignores the port, never the host.
        assert!(!redirect_uri_matches_registered(
            &reg(&["http://127.0.0.1:8080/callback"]),
            "http://localhost:56010/callback"
        ));
    }

    #[test]
    fn non_loopback_port_difference_rejected() {
        // https hosts must match exactly — no port relaxation.
        assert!(!redirect_uri_matches_registered(
            &reg(&["https://example.com:8080/cb"]),
            "https://example.com:9090/cb"
        ));
    }

    #[test]
    fn requested_non_loopback_never_relaxed() {
        // A non-loopback requested URI that isn't an exact match is rejected
        // even if a loopback entry is registered.
        assert!(!redirect_uri_matches_registered(
            &reg(&["http://localhost:8080/callback"]),
            "https://evil.com/callback"
        ));
    }
}
