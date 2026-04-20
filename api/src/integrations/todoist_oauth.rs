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

pub struct TodoistOAuth2Provider {
    authorize_url: Url,
    token_url: Url,
    client_id: String,
    client_secret: SecretBox<ClientSecret>,
    required_scopes: Vec<String>,
}

impl std::fmt::Debug for TodoistOAuth2Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("TodoistOAuth2Provider")
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("required_scopes", &self.required_scopes)
            .finish_non_exhaustive()
    }
}

impl TodoistOAuth2Provider {
    pub fn new(
        client_id: String,
        client_secret: SecretBox<ClientSecret>,
        required_scopes: Vec<String>,
    ) -> Self {
        Self {
            authorize_url: Url::parse("https://todoist.com/oauth/authorize")
                .expect("Invalid Todoist authorize URL"),
            token_url: Url::parse("https://todoist.com/oauth/access_token")
                .expect("Invalid Todoist token URL"),
            client_id,
            client_secret,
            required_scopes,
        }
    }
}

impl OAuth2Provider for TodoistOAuth2Provider {
    fn provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Todoist
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
        _raw_response: &Value,
    ) -> Result<Vec<String>, UniversalInboxError> {
        // Todoist does not include scopes in the token response.
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn provider() -> TodoistOAuth2Provider {
        TodoistOAuth2Provider::new(
            "test-client-id".to_string(),
            SecretBox::new(Box::new(ClientSecret("test-client-secret".to_string()))),
            vec!["data:read_write".to_string(), "data:delete".to_string()],
        )
    }

    #[test]
    fn test_provider_kind() {
        assert_eq!(provider().provider_kind(), IntegrationProviderKind::Todoist);
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
    fn test_extract_scopes_always_empty() {
        let raw = json!({ "access_token": "abc", "token_type": "bearer" });
        assert!(
            provider()
                .extract_registered_scopes(&raw)
                .unwrap()
                .is_empty()
        );
    }
}
