use std::sync::Arc;

use anyhow::Context;
use base64::prelude::*;
use chrono::{TimeDelta, Utc};
use jsonwebtoken::{EncodingKey, Header};
use rand::RngCore;
use ring::digest;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::{
    auth::oauth2::{OAuth2Client, TokenResponse},
    user::UserId,
};

use crate::{
    repository::{Repository, oauth2::OAuth2Repository},
    universal_inbox::UniversalInboxError,
    utils::jwt::{Claims, JWT_SIGNING_ALGO, JWTBase64EncodedSigningKeys, JWTSigningKeys},
};

const ACCESS_TOKEN_EXPIRY_SECS: u64 = 3600;
const REFRESH_TOKEN_EXPIRY_DAYS: i64 = 30;
const AUTH_CODE_EXPIRY_SECS: i64 = 60;

pub struct OAuth2Service {
    repository: Arc<Repository>,
    jwt_encoding_key: EncodingKey,
    resource_url: String,
}

impl OAuth2Service {
    pub fn new(
        repository: Arc<Repository>,
        jwt_secret_key: String,
        jwt_public_key: String,
        resource_url: String,
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

        // Verify the client exists
        let client = self
            .repository
            .get_oauth2_client_by_client_id(transaction, client_id)
            .await?
            .ok_or_else(|| UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Unknown client_id: {client_id}"),
            })?;

        // Verify the redirect_uri is registered
        if !client.redirect_uris.contains(&redirect_uri.to_string()) {
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

    #[tracing::instrument(level = "debug", skip_all, fields(client_id), err)]
    pub async fn refresh_token(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        refresh_token: &str,
        client_id: &str,
    ) -> Result<TokenResponse, UniversalInboxError> {
        let token_hash = hash_token(refresh_token);

        let stored_token = self
            .repository
            .get_refresh_token_by_hash(transaction, &token_hash)
            .await?
            .ok_or_else(|| UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "Invalid refresh token".to_string(),
            })?;

        // Verify client_id matches
        if stored_token.client_id != client_id {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "client_id mismatch".to_string(),
            });
        }

        // Verify token has not expired
        if let Some(expires_at) = stored_token.expires_at
            && expires_at < Utc::now()
        {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "Refresh token has expired".to_string(),
            });
        }

        // Revoke the old refresh token (rotation)
        self.repository
            .revoke_refresh_token(transaction, &token_hash)
            .await?;

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
    rand::rng().fill_bytes(&mut bytes);
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
