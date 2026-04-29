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

pub struct LinearOAuth2Provider {
    authorize_url: Url,
    token_url: Url,
    client_id: String,
    client_secret: SecretBox<ClientSecret>,
    required_scopes: Vec<String>,
}

impl std::fmt::Debug for LinearOAuth2Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("LinearOAuth2Provider")
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("required_scopes", &self.required_scopes)
            .finish_non_exhaustive()
    }
}

impl LinearOAuth2Provider {
    pub fn new(
        client_id: String,
        client_secret: SecretBox<ClientSecret>,
        required_scopes: Vec<String>,
    ) -> Self {
        Self {
            authorize_url: Url::parse("https://linear.app/oauth/authorize")
                .expect("Invalid Linear authorize URL"),
            token_url: Url::parse("https://api.linear.app/oauth/token")
                .expect("Invalid Linear token URL"),
            client_id,
            client_secret,
            required_scopes,
        }
    }
}

impl OAuth2Provider for LinearOAuth2Provider {
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
        &self.client_id
    }

    fn client_secret(&self) -> &SecretBox<ClientSecret> {
        &self.client_secret
    }

    fn required_scopes(&self) -> &[String] {
        &self.required_scopes
    }

    fn supports_pkce(&self) -> bool {
        true
    }

    fn scope_delimiter(&self) -> &'static str {
        ","
    }

    fn extract_registered_scopes(
        &self,
        raw_response: &Value,
    ) -> Result<Vec<String>, UniversalInboxError> {
        // Linear returns scopes as a JSON array in the token response
        if let Some(scopes) = raw_response.get("scope").and_then(|s| s.as_array()) {
            Ok(scopes
                .iter()
                .filter_map(|scope| Some(scope.as_str()?.to_string()))
                .collect())
        } else if let Some(scope_str) = raw_response.get("scope").and_then(|s| s.as_str()) {
            // Fallback: scopes as comma-separated string
            Ok(scope_str.split(',').map(|s| s.trim().to_string()).collect())
        } else {
            Ok(vec![])
        }
    }

    fn extract_provider_user_id(&self, _raw_response: &Value) -> Option<String> {
        // Linear doesn't return the user ID in the token response
        None
    }

    fn extract_provider_context(
        &self,
        _raw_response: &Value,
    ) -> Option<IntegrationConnectionContext> {
        // Linear doesn't have a workspace-level context like Slack's team_id
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn provider() -> LinearOAuth2Provider {
        LinearOAuth2Provider::new(
            "test-client-id".to_string(),
            SecretBox::new(Box::new(ClientSecret("test-client-secret".to_string()))),
            vec!["read".to_string(), "write".to_string()],
        )
    }

    #[test]
    fn test_provider_kind() {
        assert_eq!(provider().provider_kind(), IntegrationProviderKind::Linear);
    }

    #[test]
    fn test_supports_pkce() {
        assert!(provider().supports_pkce());
    }
    #[test]
    fn test_extract_scopes_from_array() {
        let raw = json!({ "scope": ["read", "write"] });
        let scopes = provider().extract_registered_scopes(&raw).unwrap();
        assert_eq!(scopes, vec!["read", "write"]);
    }

    #[test]
    fn test_extract_scopes_from_string() {
        let raw = json!({ "scope": "read,write" });
        let scopes = provider().extract_registered_scopes(&raw).unwrap();
        assert_eq!(scopes, vec!["read", "write"]);
    }

    #[test]
    fn test_extract_scopes_missing() {
        let raw = json!({});
        let scopes = provider().extract_registered_scopes(&raw).unwrap();
        assert!(scopes.is_empty());
    }

    #[test]
    fn test_extract_provider_user_id_is_none() {
        let raw = json!({ "access_token": "token" });
        assert!(provider().extract_provider_user_id(&raw).is_none());
    }

    #[test]
    fn test_extract_provider_context_is_none() {
        let raw = json!({ "access_token": "token" });
        assert!(provider().extract_provider_context(&raw).is_none());
    }
}
