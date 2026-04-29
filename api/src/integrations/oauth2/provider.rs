use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use http::HeaderMap;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Extension};
use reqwest_tracing::{DisableOtelPropagation, SpanBackendWithUrl, TracingMiddleware};
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;
use universal_inbox::integration_connection::provider::{
    IntegrationConnectionContext, IntegrationProviderKind,
};
use url::Url;

use crate::{
    integrations::{
        APP_USER_AGENT,
        oauth2::{AccessToken, AuthorizationCode, ClientSecret, PkceVerifier, RefreshToken},
    },
    universal_inbox::UniversalInboxError,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeOAuthTokenResponse {
    pub token_type: Option<String>,
    pub expires_in: Option<i64>,
    #[serde(flatten)]
    pub extra: Value,
}

/// Response from a token exchange or refresh operation.
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: SecretBox<AccessToken>,
    pub refresh_token: Option<SecretBox<RefreshToken>>,
    pub token_type: Option<String>,
    pub expires_in: Option<i64>,
    #[serde(flatten)]
    pub extra: Value,
}

impl OAuthTokenResponse {
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_in
            .and_then(TimeDelta::try_seconds)
            .map(|delta| Utc::now() + delta)
    }

    pub fn as_safe_token_response(&self) -> SafeOAuthTokenResponse {
        SafeOAuthTokenResponse {
            token_type: self.token_type.clone(),
            expires_in: self.expires_in,
            extra: self.extra.clone(),
        }
    }
}

/// Configuration for an OAuth2 provider that manages its own token lifecycle.
pub trait OAuth2Provider: Send + Sync + std::fmt::Debug {
    fn provider_kind(&self) -> IntegrationProviderKind;
    fn authorize_url(&self) -> &Url;
    fn token_url(&self) -> &Url;
    fn client_id(&self) -> &str;
    fn client_secret(&self) -> &SecretBox<ClientSecret>;
    fn required_scopes(&self) -> &[String];
    fn supports_pkce(&self) -> bool;

    /// Parse the token response and extract registered scopes.
    fn extract_registered_scopes(
        &self,
        raw_response: &Value,
    ) -> Result<Vec<String>, UniversalInboxError>;

    /// Parse the token response and extract the provider user ID (if available).
    fn extract_provider_user_id(&self, raw_response: &Value) -> Option<String>;

    /// Parse the token response and extract the provider context (if available).
    fn extract_provider_context(
        &self,
        raw_response: &Value,
    ) -> Option<IntegrationConnectionContext>;

    /// Extra query params to append to the authorize URL (e.g. Google's
    /// `access_type=offline` / `prompt=consent`). Default: none.
    fn extra_authorize_params(&self) -> Vec<(&'static str, &'static str)> {
        Vec::new()
    }

    /// Delimiter used when joining scopes in the authorize URL scope param.
    /// Defaults to "," (Linear / GitHub). Providers using RFC 6749 space
    /// separation override to " ".
    fn scope_delimiter(&self) -> &'static str {
        ","
    }

    /// Name of the scope query parameter. Defaults to "scope". Slack with
    /// `use_as_oauth_user_scopes = true` overrides to "user_scope".
    fn scope_param_name(&self) -> &'static str {
        "scope"
    }

    /// Extra HTTP headers to send with token exchange / refresh requests
    /// (e.g. GitHub requires `Accept: application/json`). Default: none.
    fn token_request_headers(&self) -> HeaderMap {
        HeaderMap::new()
    }

    /// Parse the raw token endpoint response body into an `OAuthTokenResponse`.
    /// Default implementation parses standard OAuth2 JSON; providers with
    /// non-standard response shapes (e.g. Slack v2) can override.
    fn parse_token_response(&self, body: &str) -> Result<OAuthTokenResponse, UniversalInboxError> {
        serde_json::from_str(body)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, body.to_string()))
    }

    /// Return a copy of the raw provider response with secret material stripped
    /// (access tokens, refresh tokens, id tokens). The result is what gets
    /// persisted to `oauth_credential.raw_response`.
    ///
    /// Default: strip well-known top-level OAuth2 token fields. Providers with
    /// non-standard shapes (e.g. Slack) MUST override to also strip nested
    /// occurrences.
    fn sanitize_raw_response(&self, raw: &Value) -> Value {
        let mut cleaned = raw.clone();
        if let Some(obj) = cleaned.as_object_mut() {
            for key in ["access_token", "refresh_token", "id_token"] {
                obj.remove(key);
            }
        }
        cleaned
    }
}

/// Service that executes OAuth2 flows (authorization URL generation, code exchange,
/// token refresh) using a given `OAuth2Provider`.
#[derive(Clone, Debug)]
pub struct OAuth2FlowService {
    client: ClientWithMiddleware,
    redirect_uri: Url,
}

impl OAuth2FlowService {
    pub fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    pub fn new(redirect_uri: Url) -> Result<OAuth2FlowService, UniversalInboxError> {
        let reqwest_client = reqwest_middleware::reqwest::Client::builder()
            .user_agent(APP_USER_AGENT)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to build OAuth2 HTTP client")?;
        let client = ClientBuilder::new(reqwest_client)
            .with_init(Extension(DisableOtelPropagation))
            .with(TracingMiddleware::<SpanBackendWithUrl>::new())
            .build();
        Ok(OAuth2FlowService {
            client,
            redirect_uri,
        })
    }

    pub async fn exchange_code_for_token(
        &self,
        provider: &dyn OAuth2Provider,
        code: &SecretBox<AuthorizationCode>,
        pkce_verifier: Option<&SecretBox<PkceVerifier>>,
    ) -> Result<OAuthTokenResponse, UniversalInboxError> {
        let client_secret = provider.client_secret().expose_secret().as_str();
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code.expose_secret().as_str());
        params.insert("client_id", provider.client_id());
        params.insert("client_secret", client_secret);
        let redirect_uri_str = self.redirect_uri.to_string();
        params.insert("redirect_uri", &redirect_uri_str);

        let pkce_verifier_str;
        if let Some(verifier) = pkce_verifier {
            pkce_verifier_str = verifier.expose_secret().as_str().to_string();
            params.insert("code_verifier", &pkce_verifier_str);
        }

        debug!(
            "Exchanging authorization code for token at {}",
            provider.token_url()
        );

        let response = self
            .client
            .post(provider.token_url().as_str())
            .headers(provider.token_request_headers())
            .form(&params)
            .send()
            .await
            .context("Failed to exchange authorization code for token")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read token exchange response body")?;

        if !status.is_success() {
            return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Token exchange failed with status {status}: {body}"
            )));
        }

        provider.parse_token_response(&body)
    }

    pub async fn refresh_access_token(
        &self,
        provider: &dyn OAuth2Provider,
        refresh_token: &RefreshToken,
    ) -> Result<OAuthTokenResponse, UniversalInboxError> {
        let client_secret = provider.client_secret().expose_secret().as_str();
        let params = HashMap::from([
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", provider.client_id()),
            ("client_secret", client_secret),
        ]);

        debug!(
            "Refreshing access token at {} for {:?}",
            provider.token_url(),
            provider.provider_kind()
        );

        let response = self
            .client
            .post(provider.token_url().as_str())
            .headers(provider.token_request_headers())
            .form(&params)
            .send()
            .await
            .context("Failed to refresh access token")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read token refresh response body")?;

        if !status.is_success() {
            return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Token refresh failed with status {status}: {body}"
            )));
        }

        provider.parse_token_response(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_token_response_expires_at() {
        let response = OAuthTokenResponse {
            access_token: SecretBox::new(Box::new(AccessToken("token".to_string()))),
            refresh_token: None,
            token_type: Some("Bearer".to_string()),
            expires_in: Some(86400),
            extra: Value::Null,
        };

        let expires_at = response.expires_at().unwrap();
        let expected = Utc::now() + TimeDelta::try_seconds(86400).unwrap();
        // Allow 2 second tolerance
        assert!((expires_at - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn test_oauth_token_response_no_expiry() {
        let response = OAuthTokenResponse {
            access_token: SecretBox::new(Box::new(AccessToken("token".to_string()))),
            refresh_token: None,
            token_type: None,
            expires_in: None,
            extra: Value::Null,
        };

        assert!(response.expires_at().is_none());
    }
}
