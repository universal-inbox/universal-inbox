use std::str::FromStr;

use base64::{Engine, engine::general_purpose::STANDARD};
use email_address::EmailAddress;
use http::{HeaderMap, HeaderValue, header::AUTHORIZATION};
use reqwest_middleware::ClientWithMiddleware;
use secrecy::{ExposeSecret, SecretBox};
use serde_json::{Value, json};
use universal_inbox::{
    integration_connection::{
        integrations::notion::NotionContext,
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
    },
    pii::Pii,
};
use url::Url;

use crate::{
    integrations::oauth2::{AccessToken, ClientSecret, RefreshToken, provider::OAuth2Provider},
    universal_inbox::UniversalInboxError,
};

pub struct NotionOAuth2Provider {
    authorize_url: Url,
    token_url: Url,
    client_id: String,
    client_secret: SecretBox<ClientSecret>,
    required_scopes: Vec<String>,
    revocation_url: Url,
}

impl std::fmt::Debug for NotionOAuth2Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("NotionOAuth2Provider")
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("required_scopes", &self.required_scopes)
            .finish_non_exhaustive()
    }
}

impl NotionOAuth2Provider {
    pub fn new(
        client_id: String,
        client_secret: SecretBox<ClientSecret>,
        required_scopes: Vec<String>,
    ) -> Self {
        Self {
            authorize_url: Url::parse("https://api.notion.com/v1/oauth/authorize")
                .expect("Invalid Notion authorize URL"),
            token_url: Url::parse("https://api.notion.com/v1/oauth/token")
                .expect("Invalid Notion token URL"),
            client_id,
            client_secret,
            required_scopes,
            revocation_url: Url::parse("https://api.notion.com/v1/oauth/revoke")
                .expect("Invalid Notion revocation URL"),
        }
    }

    /// Override the grant revocation endpoint (tests point it at a mock).
    pub fn with_revocation_url(mut self, revocation_url: Url) -> Self {
        self.revocation_url = revocation_url;
        self
    }

    /// Override the token endpoint (tests point it at a mock).
    pub fn with_token_url(mut self, token_url: Url) -> Self {
        self.token_url = token_url;
        self
    }
}

impl OAuth2Provider for NotionOAuth2Provider {
    fn revocation_url(&self) -> &Url {
        &self.revocation_url
    }

    /// Notion's revocation endpoint takes the access token as a JSON `token`
    /// field, with the client credentials as HTTP Basic auth.
    fn build_revocation_request(
        &self,
        client: &ClientWithMiddleware,
        revocation_url: &Url,
        access_token: &AccessToken,
        _refresh_token: Option<&RefreshToken>,
    ) -> reqwest_middleware::RequestBuilder {
        client
            .post(revocation_url.as_str())
            .basic_auth(
                &self.client_id,
                Some(self.client_secret.expose_secret().as_str()),
            )
            .json(&json!({ "token": access_token.as_str() }))
    }

    fn provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Notion
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

    /// Notion grants access per page (chosen in its page picker), not per
    /// scope, and asks for `owner=user` on the authorize URL.
    fn extra_authorize_params(&self) -> Vec<(&'static str, &'static str)> {
        vec![("owner", "user")]
    }

    /// Notion's token endpoint authenticates the client with HTTP Basic auth.
    fn token_request_headers(&self) -> HeaderMap {
        let credentials = STANDARD.encode(format!(
            "{}:{}",
            self.client_id,
            self.client_secret.expose_secret().as_str()
        ));
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(&format!("Basic {credentials}")) {
            headers.insert(AUTHORIZATION, value);
        }
        headers
    }

    fn extract_registered_scopes(
        &self,
        _raw_response: &Value,
    ) -> Result<Vec<String>, UniversalInboxError> {
        Ok(vec![])
    }

    /// The human who installed the integration (`owner.user.id`), not the bot
    /// that `GET /v1/users/me` returns.
    fn extract_provider_user_id(&self, raw_response: &Value) -> Option<String> {
        raw_response["owner"]["user"]["id"]
            .as_str()
            .map(|id| id.to_string())
    }

    fn extract_provider_context(
        &self,
        raw_response: &Value,
    ) -> Option<IntegrationConnectionContext> {
        let workspace_id = raw_response["workspace_id"].as_str()?.to_string();
        let bot_id = raw_response["bot_id"].as_str()?.to_string();
        let user = &raw_response["owner"]["user"];
        Some(IntegrationConnectionContext::Notion(NotionContext {
            workspace_id,
            workspace_name: raw_response["workspace_name"]
                .as_str()
                .map(|name| name.to_string()),
            bot_id,
            user_display_name: user["name"].as_str().map(|name| Pii::new(name.to_string())),
            user_email_address: user["person"]["email"]
                .as_str()
                .and_then(|email| Pii::<EmailAddress>::from_str(email).ok()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn provider() -> NotionOAuth2Provider {
        NotionOAuth2Provider::new(
            "test-client-id".to_string(),
            SecretBox::new(Box::new(ClientSecret("test-client-secret".to_string()))),
            vec![],
        )
    }

    fn token_response() -> Value {
        json!({
            "access_token": "secret_token",
            "token_type": "bearer",
            "bot_id": "bot-123",
            "workspace_id": "workspace-123",
            "workspace_name": "Acme",
            "workspace_icon": null,
            "owner": {
                "type": "user",
                "user": {
                    "object": "user",
                    "id": "user-123",
                    "name": "John Doe",
                    "avatar_url": null,
                    "type": "person",
                    "person": { "email": "john@example.com" }
                }
            },
            "duplicated_template_id": null
        })
    }

    #[test]
    fn test_provider_kind() {
        assert_eq!(provider().provider_kind(), IntegrationProviderKind::Notion);
    }

    #[test]
    fn test_authorize_params() {
        assert!(!provider().supports_pkce());
        assert_eq!(provider().extra_authorize_params(), vec![("owner", "user")]);
    }

    #[test]
    fn test_token_request_headers_use_basic_auth() {
        let headers = provider().token_request_headers();
        assert_eq!(
            headers.get(AUTHORIZATION).unwrap(),
            &format!(
                "Basic {}",
                STANDARD.encode("test-client-id:test-client-secret")
            )
        );
    }

    #[test]
    fn test_extract_scopes_is_empty() {
        let scopes = provider()
            .extract_registered_scopes(&token_response())
            .unwrap();
        assert!(scopes.is_empty());
    }

    #[test]
    fn test_extract_provider_user_id_is_the_owner() {
        assert_eq!(
            provider().extract_provider_user_id(&token_response()),
            Some("user-123".to_string())
        );
    }

    #[test]
    fn test_extract_provider_context() {
        assert_eq!(
            provider().extract_provider_context(&token_response()),
            Some(IntegrationConnectionContext::Notion(NotionContext {
                workspace_id: "workspace-123".to_string(),
                workspace_name: Some("Acme".to_string()),
                bot_id: "bot-123".to_string(),
                user_display_name: Some(Pii::new("John Doe".to_string())),
                user_email_address: Some(Pii::new(
                    EmailAddress::from_str("john@example.com").unwrap()
                )),
            }))
        );
    }

    #[test]
    fn test_extract_provider_context_without_workspace_is_none() {
        let raw = json!({ "access_token": "token", "bot_id": "bot-123" });
        assert!(provider().extract_provider_context(&raw).is_none());
    }

    #[test]
    fn test_sanitize_raw_response_strips_token() {
        let cleaned = provider().sanitize_raw_response(&token_response());
        assert!(cleaned.get("access_token").is_none());
        assert_eq!(cleaned["owner"]["user"]["id"], "user-123");
    }
}
