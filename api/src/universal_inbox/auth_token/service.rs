use std::sync::Arc;

use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use jsonwebtoken::{EncodingKey, Header};
use secrecy::Secret;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::{
    auth::auth_token::{AuthenticationToken, JWTToken, TruncatedAuthenticationToken},
    user::UserId,
};

use crate::{
    configuration::HttpSessionSettings,
    repository::{auth_token::AuthenticationTokenRepository, Repository},
    universal_inbox::UniversalInboxError,
    utils::jwt::{Claims, JWTBase64EncodedSigningKeys, JWTSigningKeys, JWT_SIGNING_ALGO},
};

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
        fields(is_session_token, user.id = user_id.to_string()),
        err
    )]
    pub async fn create_auth_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        is_session_token: bool,
        user_id: UserId,
        expire_at: Option<DateTime<Utc>>,
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
        };

        let jwt_token = Secret::new(JWTToken(
            jsonwebtoken::encode(
                &Header::new(JWT_SIGNING_ALGO),
                &claims,
                &self.jwt_encoding_key,
            )
            .context("Failed to encode JSON web token")?,
        ));
        let auth_token = self
            .repository
            .create_auth_token(
                executor,
                AuthenticationToken::new(user_id, jwt_token, Some(expire_at), is_session_token),
            )
            .await?;
        Ok(auth_token)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    pub async fn fetch_auth_tokens_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<TruncatedAuthenticationToken>, UniversalInboxError> {
        let authentication_tokens = self
            .repository
            .fetch_auth_tokens_for_user(executor, user_id, true)
            .await?;
        Ok(authentication_tokens
            .into_iter()
            .map(TruncatedAuthenticationToken::new)
            .collect())
    }
}
