use http::{HeaderMap, HeaderValue, header::ACCEPT};
use secrecy::SecretBox;
use serde_json::Value;
use universal_inbox::integration_connection::provider::{
    IntegrationConnectionContext, IntegrationProviderKind,
};
use url::Url;

use crate::{
    integrations::oauth2::{ClientSecret, provider::OAuth2Provider},
    universal_inbox::UniversalInboxError,
};

pub struct GithubOAuth2Provider {
    authorize_url: Url,
    token_url: Url,
    client_id: String,
    client_secret: SecretBox<ClientSecret>,
    required_scopes: Vec<String>,
}

impl std::fmt::Debug for GithubOAuth2Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("GithubOAuth2Provider")
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("required_scopes", &self.required_scopes)
            .finish_non_exhaustive()
    }
}

impl GithubOAuth2Provider {
    pub fn new(
        client_id: String,
        client_secret: SecretBox<ClientSecret>,
        required_scopes: Vec<String>,
    ) -> Self {
        Self {
            authorize_url: Url::parse("https://github.com/login/oauth/authorize")
                .expect("Invalid Github authorize URL"),
            token_url: Url::parse("https://github.com/login/oauth/access_token")
                .expect("Invalid Github token URL"),
            client_id,
            client_secret,
            required_scopes,
        }
    }
}

impl OAuth2Provider for GithubOAuth2Provider {
    fn provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Github
    }

    fn authorize_url(&self) -> &Url {
        &self.authorize_url
    }

    fn token_url(&self) -> &Url {
        &self.token_url
    }

    fn client_id(&self) -> &str {
        &self.client_id
    }

    fn client_secret(&self) -> &SecretBox<ClientSecret> {
        &self.client_secret
    }

    fn required_scopes(&self) -> &[String] {
        &self.required_scopes
    }

    fn supports_pkce(&self) -> bool {
        false
    }

    fn migration_url(&self) -> Option<&Url> {
        None
    }

    fn extract_registered_scopes(
        &self,
        raw_response: &Value,
    ) -> Result<Vec<String>, UniversalInboxError> {
        Ok(raw_response
            .get("scope")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string())
            .collect())
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

    fn token_request_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn provider() -> GithubOAuth2Provider {
        GithubOAuth2Provider::new(
            "test-client-id".to_string(),
            SecretBox::new(Box::new(ClientSecret("test-client-secret".to_string()))),
            vec!["repo".to_string(), "read:org".to_string()],
        )
    }

    #[test]
    fn test_provider_kind() {
        assert_eq!(provider().provider_kind(), IntegrationProviderKind::Github);
    }

    #[test]
    fn test_supports_pkce() {
        assert!(!provider().supports_pkce());
    }

    #[test]
    fn test_migration_url_is_none() {
        assert!(provider().migration_url().is_none());
    }

    #[test]
    fn test_scope_delimiter_is_comma() {
        assert_eq!(provider().scope_delimiter(), ",");
    }

    #[test]
    fn test_extract_scopes_comma_separated() {
        let raw = json!({ "scope": "repo,read:org,notifications" });
        let scopes = provider().extract_registered_scopes(&raw).unwrap();
        assert_eq!(scopes, vec!["repo", "read:org", "notifications"]);
    }

    #[test]
    fn test_extract_scopes_missing() {
        let raw = json!({});
        assert!(
            provider()
                .extract_registered_scopes(&raw)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_token_request_headers_include_accept_json() {
        let headers = provider().token_request_headers();
        assert_eq!(
            headers.get(ACCEPT).and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
    }
}
