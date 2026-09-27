use anyhow::Context;
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
    revocation_url: Url,
    provider_user_id_url: Url,
}

impl std::fmt::Debug for GoogleOAuth2Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("GoogleOAuth2Provider")
            .field("provider_kind", &self.provider_kind)
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("required_scopes", &self.required_scopes)
            .field("provider_user_id_url", &self.provider_user_id_url)
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
            revocation_url: Url::parse("https://oauth2.googleapis.com/revoke")
                .expect("Invalid Google revocation URL"),
            provider_user_id_url: build_provider_user_id_url(
                provider_kind,
                default_api_base_url(provider_kind),
            )
            .expect("Invalid Google provider user ID URL"),
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

    /// Override the base URL of the Google API the provider user ID is read
    /// from (tests point it at a mock).
    pub fn with_api_base_url(mut self, api_base_url: &str) -> Result<Self, UniversalInboxError> {
        self.provider_user_id_url = build_provider_user_id_url(self.provider_kind, api_base_url)?;
        Ok(self)
    }
}

fn default_api_base_url(provider_kind: IntegrationProviderKind) -> &'static str {
    match provider_kind {
        IntegrationProviderKind::GoogleCalendar => "https://www.googleapis.com/calendar/v3",
        IntegrationProviderKind::GoogleDrive => "https://www.googleapis.com/drive/v3",
        _ => "https://gmail.googleapis.com/gmail/v1",
    }
}

/// The token response does not identify the Google account and the
/// integrations do not request the `openid`/`email` scopes the userinfo
/// endpoint needs, so each integration reads the account email from its own
/// API, within the scopes it already has.
fn build_provider_user_id_url(
    provider_kind: IntegrationProviderKind,
    api_base_url: &str,
) -> Result<Url, UniversalInboxError> {
    let api_base_url = api_base_url.trim_end_matches('/');
    let url = match provider_kind {
        IntegrationProviderKind::GoogleCalendar => format!("{api_base_url}/calendars/primary"),
        IntegrationProviderKind::GoogleDrive => {
            format!("{api_base_url}/about?fields=user(emailAddress)")
        }
        _ => format!("{api_base_url}/users/me/profile"),
    };
    Ok(Url::parse(&url).context("Invalid Google provider user ID URL")?)
}

impl OAuth2Provider for GoogleOAuth2Provider {
    fn revocation_url(&self) -> &Url {
        &self.revocation_url
    }
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

    fn provider_user_id_url(&self) -> Option<&Url> {
        Some(&self.provider_user_id_url)
    }

    fn parse_provider_user_id_response(&self, response: &Value) -> Option<String> {
        // The primary calendar ID is the account email address.
        let email_address = match self.provider_kind {
            IntegrationProviderKind::GoogleCalendar => response.get("id"),
            IntegrationProviderKind::GoogleDrive => response.pointer("/user/emailAddress"),
            _ => response.get("emailAddress"),
        }?
        .as_str()?;
        (!email_address.is_empty()).then(|| email_address.to_lowercase())
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

    fn provider(provider_kind: IntegrationProviderKind) -> GoogleOAuth2Provider {
        GoogleOAuth2Provider::new(
            provider_kind,
            "cid".to_string(),
            SecretBox::new(Box::new(ClientSecret("cs".to_string()))),
            vec![],
        )
    }

    #[test]
    fn test_provider_user_id_url_per_kind() {
        let url = |kind| provider(kind).provider_user_id_url().unwrap().to_string();
        assert_eq!(
            url(IntegrationProviderKind::GoogleMail),
            "https://gmail.googleapis.com/gmail/v1/users/me/profile"
        );
        assert_eq!(
            url(IntegrationProviderKind::GoogleCalendar),
            "https://www.googleapis.com/calendar/v3/calendars/primary"
        );
        assert_eq!(
            url(IntegrationProviderKind::GoogleDrive),
            "https://www.googleapis.com/drive/v3/about?fields=user(emailAddress)"
        );
    }

    #[test]
    fn test_provider_user_id_url_with_api_base_url() {
        let provider = provider(IntegrationProviderKind::GoogleCalendar)
            .with_api_base_url("http://127.0.0.1:1234/")
            .unwrap();
        assert_eq!(
            provider.provider_user_id_url().unwrap().as_str(),
            "http://127.0.0.1:1234/calendars/primary"
        );
    }

    #[test]
    fn test_parse_provider_user_id_response_per_kind() {
        assert_eq!(
            provider(IntegrationProviderKind::GoogleMail)
                .parse_provider_user_id_response(&json!({ "emailAddress": "Jane@Example.com" })),
            Some("jane@example.com".to_string())
        );
        assert_eq!(
            provider(IntegrationProviderKind::GoogleCalendar)
                .parse_provider_user_id_response(&json!({ "id": "jane@example.com" })),
            Some("jane@example.com".to_string())
        );
        assert_eq!(
            provider(IntegrationProviderKind::GoogleDrive).parse_provider_user_id_response(
                &json!({ "user": { "emailAddress": "jane@example.com" } })
            ),
            Some("jane@example.com".to_string())
        );
    }

    #[test]
    fn test_parse_provider_user_id_response_missing_email() {
        let provider = provider(IntegrationProviderKind::GoogleMail);
        assert_eq!(provider.parse_provider_user_id_response(&json!({})), None);
        assert_eq!(
            provider.parse_provider_user_id_response(&json!({ "emailAddress": "" })),
            None
        );
    }
}
