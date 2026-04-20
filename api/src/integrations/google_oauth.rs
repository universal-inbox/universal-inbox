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

/// Shared OAuth2 provider for Google Mail, Calendar, and Drive.
/// Google uses the same OAuth2 endpoints across all its APIs; only the
/// `provider_kind` differs. The requested scopes (set via config) determine
/// which API the issued token can access.
pub struct GoogleOAuth2Provider {
    provider_kind: IntegrationProviderKind,
    authorize_url: Url,
    token_url: Url,
    client_id: String,
    client_secret: SecretBox<ClientSecret>,
    required_scopes: Vec<String>,
}

impl std::fmt::Debug for GoogleOAuth2Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("GoogleOAuth2Provider")
            .field("provider_kind", &self.provider_kind)
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("required_scopes", &self.required_scopes)
            .finish_non_exhaustive()
    }
}

impl GoogleOAuth2Provider {
    pub fn new(
        provider_kind: IntegrationProviderKind,
        client_id: String,
        client_secret: SecretBox<ClientSecret>,
        required_scopes: Vec<String>,
    ) -> Self {
        Self {
            provider_kind,
            authorize_url: Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
                .expect("Invalid Google authorize URL"),
            token_url: Url::parse("https://oauth2.googleapis.com/token")
                .expect("Invalid Google token URL"),
            client_id,
            client_secret,
            required_scopes,
        }
    }
}

impl OAuth2Provider for GoogleOAuth2Provider {
    fn provider_kind(&self) -> IntegrationProviderKind {
        self.provider_kind
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

    fn migration_url(&self) -> Option<&Url> {
        None
    }

    fn scope_delimiter(&self) -> &'static str {
        " "
    }

    fn extra_authorize_params(&self) -> Vec<(&'static str, &'static str)> {
        // access_type=offline + prompt=consent ensures Google returns a refresh_token
        // on every authorization (not just the first), and allows long-lived refresh.
        vec![("access_type", "offline"), ("prompt", "consent")]
    }

    fn extract_registered_scopes(
        &self,
        raw_response: &Value,
    ) -> Result<Vec<String>, UniversalInboxError> {
        Ok(raw_response
            .get("scope")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .split(' ')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect())
    }

    fn extract_provider_user_id(&self, _raw_response: &Value) -> Option<String> {
        None
    }

    fn extract_provider_context(
        &self,
        _raw_response: &Value,
    ) -> Option<IntegrationConnectionContext> {
        // Google Mail / Drive contexts (user email, labels, etc.) are populated
        // by the per-integration sync services after the first successful API call,
        // not from the OAuth token response.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn mail_provider() -> GoogleOAuth2Provider {
        GoogleOAuth2Provider::new(
            IntegrationProviderKind::GoogleMail,
            "test-client-id".to_string(),
            SecretBox::new(Box::new(ClientSecret("test-client-secret".to_string()))),
            vec!["https://www.googleapis.com/auth/gmail.modify".to_string()],
        )
    }

    #[test]
    fn test_provider_kind_varies() {
        assert_eq!(
            mail_provider().provider_kind(),
            IntegrationProviderKind::GoogleMail
        );
        let cal = GoogleOAuth2Provider::new(
            IntegrationProviderKind::GoogleCalendar,
            "cid".to_string(),
            SecretBox::new(Box::new(ClientSecret("cs".to_string()))),
            vec![],
        );
        assert_eq!(cal.provider_kind(), IntegrationProviderKind::GoogleCalendar);
    }

    #[test]
    fn test_supports_pkce() {
        assert!(mail_provider().supports_pkce());
    }

    #[test]
    fn test_migration_url_is_none() {
        assert!(mail_provider().migration_url().is_none());
    }

    #[test]
    fn test_scope_delimiter_is_space() {
        assert_eq!(mail_provider().scope_delimiter(), " ");
    }

    #[test]
    fn test_extra_authorize_params_include_offline_and_consent() {
        let params = mail_provider().extra_authorize_params();
        assert!(params.contains(&("access_type", "offline")));
        assert!(params.contains(&("prompt", "consent")));
    }

    #[test]
    fn test_extract_scopes_space_separated() {
        let raw = json!({
            "scope": "https://www.googleapis.com/auth/gmail.modify https://www.googleapis.com/auth/userinfo.email"
        });
        let scopes = mail_provider().extract_registered_scopes(&raw).unwrap();
        assert_eq!(
            scopes,
            vec![
                "https://www.googleapis.com/auth/gmail.modify".to_string(),
                "https://www.googleapis.com/auth/userinfo.email".to_string(),
            ]
        );
    }

    #[test]
    fn test_extract_scopes_missing() {
        let raw = json!({});
        assert!(
            mail_provider()
                .extract_registered_scopes(&raw)
                .unwrap()
                .is_empty()
        );
    }
}
