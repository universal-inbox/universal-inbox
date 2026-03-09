use std::collections::HashMap;

use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Extension};
use reqwest_tracing::{DisableOtelPropagation, SpanBackendWithUrl, TracingMiddleware};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;
use universal_inbox::integration_connection::provider::{
    IntegrationConnectionContext, IntegrationProviderKind,
};
use url::Url;

use crate::{integrations::APP_USER_AGENT, universal_inbox::UniversalInboxError};

/// Response from a token exchange or refresh operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
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
}

/// Configuration for an OAuth2 provider that manages its own token lifecycle
/// (as opposed to using Nango).
pub trait OAuth2Provider: Send + Sync + std::fmt::Debug {
    fn provider_kind(&self) -> IntegrationProviderKind;
    fn authorize_url(&self) -> &Url;
    fn token_url(&self) -> &Url;
    fn client_id(&self) -> &str;
    fn client_secret(&self) -> &str;
    fn required_scopes(&self) -> &[String];
    fn supports_pkce(&self) -> bool;

    /// URL for migrating existing long-lived tokens to short-lived + refresh token.
    /// Returns `None` if this provider doesn't support token migration.
    fn migration_url(&self) -> Option<&Url>;

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
}

/// Service that executes OAuth2 flows (authorization URL generation, code exchange,
/// token refresh) using a given `OAuth2Provider`.
#[derive(Clone, Debug)]
pub struct OAuth2FlowService {
    client: ClientWithMiddleware,
}

impl OAuth2FlowService {
    pub fn new() -> Result<OAuth2FlowService, UniversalInboxError> {
        let reqwest_client = reqwest_middleware::reqwest::Client::builder()
            .user_agent(APP_USER_AGENT)
            .build()
            .context("Failed to build OAuth2 HTTP client")?;
        let client = ClientBuilder::new(reqwest_client)
            .with_init(Extension(DisableOtelPropagation))
            .with(TracingMiddleware::<SpanBackendWithUrl>::new())
            .build();
        Ok(OAuth2FlowService { client })
    }

    /// Build an authorization URL for the given provider.
    /// Returns `(authorization_url, pkce_verifier)`.
    pub fn generate_authorization_url(
        &self,
        provider: &dyn OAuth2Provider,
        redirect_uri: &Url,
        state: &str,
        pkce_challenge: Option<&str>,
        pkce_method: Option<&str>,
    ) -> Url {
        let mut url = provider.authorize_url().clone();
        {
            let mut params = url.query_pairs_mut();
            params.append_pair("response_type", "code");
            params.append_pair("client_id", provider.client_id());
            params.append_pair("redirect_uri", redirect_uri.as_str());
            params.append_pair("state", state);

            let scopes = provider.required_scopes();
            if !scopes.is_empty() {
                params.append_pair("scope", &scopes.join(","));
            }

            if let (Some(challenge), Some(method)) = (pkce_challenge, pkce_method) {
                params.append_pair("code_challenge", challenge);
                params.append_pair("code_challenge_method", method);
            }
        }
        url
    }

    /// Exchange an authorization code for tokens.
    pub async fn exchange_code_for_token(
        &self,
        provider: &dyn OAuth2Provider,
        code: &str,
        redirect_uri: &Url,
        pkce_verifier: Option<&str>,
    ) -> Result<OAuthTokenResponse, UniversalInboxError> {
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("client_id", provider.client_id());
        params.insert("client_secret", provider.client_secret());
        let redirect_uri_str = redirect_uri.to_string();
        params.insert("redirect_uri", &redirect_uri_str);

        let pkce_verifier_owned;
        if let Some(verifier) = pkce_verifier {
            pkce_verifier_owned = verifier.to_string();
            params.insert("code_verifier", &pkce_verifier_owned);
        }

        debug!(
            "Exchanging authorization code for token at {}",
            provider.token_url()
        );

        let response = self
            .client
            .post(provider.token_url().as_str())
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

        serde_json::from_str(&body)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, body))
    }

    /// Refresh an access token using a refresh token.
    pub async fn refresh_access_token(
        &self,
        provider: &dyn OAuth2Provider,
        refresh_token: &str,
    ) -> Result<OAuthTokenResponse, UniversalInboxError> {
        let params = HashMap::from([
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", provider.client_id()),
            ("client_secret", provider.client_secret()),
        ]);

        debug!(
            "Refreshing access token at {} for {:?}",
            provider.token_url(),
            provider.provider_kind()
        );

        let response = self
            .client
            .post(provider.token_url().as_str())
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

        serde_json::from_str(&body)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, body))
    }

    /// Migrate an existing long-lived token to short-lived + refresh token.
    pub async fn migrate_old_token(
        &self,
        provider: &dyn OAuth2Provider,
        old_access_token: &str,
    ) -> Result<OAuthTokenResponse, UniversalInboxError> {
        let migration_url = provider.migration_url().ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Provider {:?} does not support token migration",
                provider.provider_kind()
            ))
        })?;

        let params = HashMap::from([
            ("access_token", old_access_token),
            ("client_id", provider.client_id()),
            ("client_secret", provider.client_secret()),
        ]);

        debug!(
            "Migrating old token at {} for {:?}",
            migration_url,
            provider.provider_kind()
        );

        let response = self
            .client
            .post(migration_url.as_str())
            .form(&params)
            .send()
            .await
            .context("Failed to migrate old token")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read token migration response body")?;

        if !status.is_success() {
            return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Token migration failed with status {status}: {body}"
            )));
        }

        serde_json::from_str(&body)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::oauth2::provider::tests::test_provider::TestOAuth2Provider;

    mod test_provider {
        use super::*;

        #[derive(Debug)]
        pub struct TestOAuth2Provider {
            pub authorize_url: Url,
            pub token_url: Url,
        }

        impl Default for TestOAuth2Provider {
            fn default() -> Self {
                Self {
                    authorize_url: Url::parse("https://example.com/oauth/authorize").unwrap(),
                    token_url: Url::parse("https://example.com/oauth/token").unwrap(),
                }
            }
        }

        impl OAuth2Provider for TestOAuth2Provider {
            fn provider_kind(&self) -> IntegrationProviderKind {
                IntegrationProviderKind::Linear
            }
            fn authorize_url(&self) -> &Url {
                &self.authorize_url
            }
            fn token_url(&self) -> &Url {
                &self.token_url
            }
            fn client_id(&self) -> &str {
                "test-client-id"
            }
            fn client_secret(&self) -> &str {
                "test-client-secret"
            }
            fn required_scopes(&self) -> &[String] {
                &[]
            }
            fn supports_pkce(&self) -> bool {
                true
            }
            fn migration_url(&self) -> Option<&Url> {
                None
            }
            fn extract_registered_scopes(
                &self,
                _raw_response: &Value,
            ) -> Result<Vec<String>, UniversalInboxError> {
                Ok(vec![])
            }
            fn extract_provider_user_id(&self, _raw_response: &Value) -> Option<String> {
                None
            }
            fn extract_provider_context(
                &self,
                _raw_response: &Value,
            ) -> Option<IntegrationConnectionContext> {
                None
            }
        }
    }

    #[test]
    fn test_generate_authorization_url_basic() {
        let service = OAuth2FlowService::new().unwrap();
        let provider = TestOAuth2Provider::default();
        let redirect_uri = Url::parse("https://app.example.com/oauth/callback").unwrap();

        let url = service.generate_authorization_url(
            &provider,
            &redirect_uri,
            "random-state",
            None,
            None,
        );

        assert!(url.as_str().contains("response_type=code"));
        assert!(url.as_str().contains("client_id=test-client-id"));
        assert!(url.as_str().contains("state=random-state"));
        assert!(
            url.as_str()
                .contains("redirect_uri=https%3A%2F%2Fapp.example.com%2Foauth%2Fcallback")
        );
    }

    #[test]
    fn test_generate_authorization_url_with_pkce() {
        let service = OAuth2FlowService::new().unwrap();
        let provider = TestOAuth2Provider::default();
        let redirect_uri = Url::parse("https://app.example.com/oauth/callback").unwrap();

        let url = service.generate_authorization_url(
            &provider,
            &redirect_uri,
            "random-state",
            Some("challenge-value"),
            Some("S256"),
        );

        assert!(url.as_str().contains("code_challenge=challenge-value"));
        assert!(url.as_str().contains("code_challenge_method=S256"));
    }

    #[test]
    fn test_oauth_token_response_expires_at() {
        let response = OAuthTokenResponse {
            access_token: "token".to_string(),
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
            access_token: "token".to_string(),
            refresh_token: None,
            token_type: None,
            expires_in: None,
            extra: Value::Null,
        };

        assert!(response.expires_at().is_none());
    }
}
