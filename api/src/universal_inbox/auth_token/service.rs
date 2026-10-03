use std::sync::Arc;

use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use futures::{FutureExt, future::LocalBoxFuture};
use jsonwebtoken::{EncodingKey, Header};
use secrecy::SecretBox;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use uuid::Uuid;

use universal_inbox::{
    auth::auth_token::{
        AuthenticationToken, AuthenticationTokenId, JWTToken, TruncatedAuthenticationToken,
    },
    user::UserId,
};

use crate::observability::attr;
use crate::{
    configuration::HttpSessionSettings,
    middlewares::jwt_auth::{BearerTokenChecker, JWT},
    repository::{
        Repository,
        auth_token::{AuthenticationTokenRepository, hash_jwt_token},
    },
    universal_inbox::UniversalInboxError,
    utils::jwt::{Claims, JWT_SIGNING_ALGO, JWTBase64EncodedSigningKeys, JWTSigningKeys},
};

/// [`BearerTokenChecker`] backed by the stored `authentication_token` rows.
pub struct StoredBearerTokenChecker(pub Arc<RwLock<AuthenticationTokenService>>);

impl BearerTokenChecker for StoredBearerTokenChecker {
    fn is_active(&self, jwt: JWT) -> LocalBoxFuture<'static, Result<bool, UniversalInboxError>> {
        let service = self.0.clone();
        async move { service.read().await.is_bearer_token_active(&jwt).await }.boxed_local()
    }
}

pub struct AuthenticationTokenService {
    repository: Arc<Repository>,
    http_session_settings: HttpSessionSettings,
    jwt_encoding_key: EncodingKey,
}

impl AuthenticationTokenService {
    pub fn new(repository: Arc<Repository>, http_session_settings: HttpSessionSettings) -> Self {
        let jwt_signing_keys =
            JWTSigningKeys::load_from_base64_encoded_keys(JWTBase64EncodedSigningKeys {
                secret_key: http_session_settings.jwt_secret_key.clone(),
                public_key: http_session_settings.jwt_public_key.clone(),
            })
            .expect("Failed to load JWT signing keys");
        Self {
            repository,
            http_session_settings,
            jwt_encoding_key: jwt_signing_keys.encoding_key.clone(),
        }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::AUTH_TOKEN_IS_SESSION } = is_session_token, { attr::USER_ID } = user_id.to_string())
    )]
    pub async fn create_auth_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        is_session_token: bool,
        user_id: UserId,
        expire_at: Option<DateTime<Utc>>,
        store: bool,
    ) -> Result<AuthenticationToken, UniversalInboxError> {
        let expire_at = expire_at.unwrap_or_else(|| {
            Utc::now()
                + TimeDelta::try_days(self.http_session_settings.jwt_token_expiration_in_days)
                    .unwrap_or_else(|| {
                        panic!(
                            "Invalid `jwt_token_expiration_in_days` value: {}",
                            self.http_session_settings.jwt_token_expiration_in_days
                        )
                    })
        });
        let claims = Claims {
            iat: Utc::now().timestamp() as usize,
            exp: expire_at.timestamp() as usize,
            sub: user_id.to_string(),
            jti: Uuid::new_v4().to_string(),
            aud: None,
            scope: None,
            client_id: None,
        };

        let jwt_token = SecretBox::new(Box::new(JWTToken(
            jsonwebtoken::encode(
                &Header::new(JWT_SIGNING_ALGO),
                &claims,
                &self.jwt_encoding_key,
            )
            .context("Failed to encode JSON web token")?,
        )));
        let auth_token =
            AuthenticationToken::new(user_id, jwt_token, Some(expire_at), is_session_token);
        if store {
            self.repository
                .create_auth_token(executor, auth_token)
                .await
        } else {
            Ok(auth_token)
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn fetch_auth_tokens_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<TruncatedAuthenticationToken>, UniversalInboxError> {
        self.repository
            .fetch_auth_tokens_for_user(executor, user_id, true)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string(), { attr::AUTH_TOKEN_ID } = auth_token_id.to_string())
    )]
    pub async fn revoke_auth_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        auth_token_id: AuthenticationTokenId,
    ) -> Result<bool, UniversalInboxError> {
        self.repository
            .revoke_auth_token(executor, user_id, auth_token_id)
            .await
    }

    /// Whether a validly signed bearer token may still authenticate.
    ///
    /// Stored tokens (API keys created from the settings page or the CLI) are
    /// looked up by digest and must be neither revoked nor expired. Tokens
    /// that were never stored (session JWTs, short-lived OAuth2 access
    /// tokens) are accepted on their signature and `exp` alone, as before.
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn is_bearer_token_active(&self, jwt: &JWT) -> Result<bool, UniversalInboxError> {
        let mut transaction = self.repository.begin().await?;
        let status = self
            .repository
            .get_auth_token_status_by_hash(&mut transaction, &hash_jwt_token(&jwt.0))
            .await?;
        transaction
            .commit()
            .await
            .context("Failed to commit while checking authentication token status")?;

        Ok(match status {
            None => true,
            Some((is_revoked, expire_at)) => {
                !is_revoked && expire_at.is_none_or(|expire_at| expire_at > Utc::now())
            }
        })
    }
}
