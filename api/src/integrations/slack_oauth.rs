use secrecy::SecretBox;
use serde_json::Value;
use slack_morphism::SlackTeamId;
use universal_inbox::integration_connection::{
    integrations::slack::SlackContext,
    provider::{IntegrationConnectionContext, IntegrationProviderKind},
};
use url::Url;

use crate::{
    integrations::oauth2::{
        AccessToken, ClientSecret, RefreshToken,
        provider::{OAuth2Provider, OAuthTokenResponse},
    },
    universal_inbox::UniversalInboxError,
};

pub struct SlackOAuth2Provider {
    authorize_url: Url,
    token_url: Url,
    client_id: String,
    client_secret: SecretBox<ClientSecret>,
    required_scopes: Vec<String>,
}

impl std::fmt::Debug for SlackOAuth2Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("SlackOAuth2Provider")
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("required_scopes", &self.required_scopes)
            .finish_non_exhaustive()
    }
}

impl SlackOAuth2Provider {
    pub fn new(
        client_id: String,
        client_secret: SecretBox<ClientSecret>,
        required_scopes: Vec<String>,
    ) -> Self {
        Self {
            authorize_url: Url::parse("https://slack.com/oauth/v2/authorize")
                .expect("Invalid Slack authorize URL"),
            token_url: Url::parse("https://slack.com/api/oauth.v2.access")
                .expect("Invalid Slack token URL"),
            client_id,
            client_secret,
            required_scopes,
        }
    }
}

impl OAuth2Provider for SlackOAuth2Provider {
    fn provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Slack
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

    fn scope_param_name(&self) -> &'static str {
        // Slack requested scopes are user scopes (use_as_oauth_user_scopes = true)
        "user_scope"
    }

    fn extract_registered_scopes(
        &self,
        raw_response: &Value,
    ) -> Result<Vec<String>, UniversalInboxError> {
        Ok(raw_response["authed_user"]["scope"]
            .as_str()
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect())
    }

    fn extract_provider_user_id(&self, raw_response: &Value) -> Option<String> {
        raw_response["authed_user"]["id"]
            .as_str()
            .map(|s| s.to_string())
    }

    fn extract_provider_context(
        &self,
        raw_response: &Value,
    ) -> Option<IntegrationConnectionContext> {
        let team_id = raw_response["team"]["id"].as_str()?.to_string();
        Some(IntegrationConnectionContext::Slack(SlackContext {
            team_id: SlackTeamId(team_id),
            extension_credentials: vec![],
            last_extension_heartbeat_at: None,
        }))
    }

    fn sanitize_raw_response(&self, raw: &Value) -> Value {
        let mut cleaned = raw.clone();
        if let Some(obj) = cleaned.as_object_mut() {
            for key in ["access_token", "refresh_token", "id_token"] {
                obj.remove(key);
            }
            if let Some(authed) = obj.get_mut("authed_user").and_then(|v| v.as_object_mut()) {
                for key in ["access_token", "refresh_token", "id_token"] {
                    authed.remove(key);
                }
            }
        }
        cleaned
    }

    fn parse_token_response(&self, body: &str) -> Result<OAuthTokenResponse, UniversalInboxError> {
        // Slack OAuth v2 returns `{ "ok": true, "authed_user": { "access_token": "...", ... }, ... }`
        // when user scopes are requested. The bot token (top-level access_token) may or may not
        // be present. We persist the user token.
        let raw: Value = serde_json::from_str(body)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, body.to_string()))?;

        if raw.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err_msg = raw
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Slack token exchange failed: {err_msg} (body: {body})"
            )));
        }

        let access_token = raw["authed_user"]["access_token"]
            .as_str()
            .or_else(|| raw["access_token"].as_str())
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow::anyhow!(
                    "Slack token response missing access_token (body: {body})"
                ))
            })?
            .to_string();

        let refresh_token = raw["authed_user"]["refresh_token"]
            .as_str()
            .or_else(|| raw["refresh_token"].as_str())
            .map(|t| SecretBox::new(Box::new(RefreshToken(t.to_string()))));

        let expires_in = raw["authed_user"]["expires_in"]
            .as_i64()
            .or_else(|| raw["expires_in"].as_i64());

        Ok(OAuthTokenResponse {
            access_token: SecretBox::new(Box::new(AccessToken(access_token))),
            refresh_token,
            token_type: raw
                .get("token_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            expires_in,
            extra: raw,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use secrecy::ExposeSecret;
    use serde_json::json;

    fn provider() -> SlackOAuth2Provider {
        SlackOAuth2Provider::new(
            "test-client-id".to_string(),
            SecretBox::new(Box::new(ClientSecret("test-client-secret".to_string()))),
            vec!["channels:read".to_string(), "chat:write".to_string()],
        )
    }

    #[test]
    fn test_provider_kind() {
        assert_eq!(provider().provider_kind(), IntegrationProviderKind::Slack);
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
    fn test_scope_param_name() {
        assert_eq!(provider().scope_param_name(), "user_scope");
    }

    #[test]
    fn test_extract_scopes() {
        let raw = json!({
            "authed_user": { "scope": "channels:read,chat:write,users:read" }
        });
        let scopes = provider().extract_registered_scopes(&raw).unwrap();
        assert_eq!(scopes, vec!["channels:read", "chat:write", "users:read"]);
    }

    #[test]
    fn test_extract_provider_user_id() {
        let raw = json!({ "authed_user": { "id": "U123456" } });
        assert_eq!(
            provider().extract_provider_user_id(&raw),
            Some("U123456".to_string())
        );
    }

    #[test]
    fn test_extract_provider_context_returns_team_id() {
        let raw = json!({ "team": { "id": "T98765" } });
        match provider().extract_provider_context(&raw) {
            Some(IntegrationConnectionContext::Slack(ctx)) => {
                assert_eq!(ctx.team_id.0, "T98765");
            }
            other => panic!("expected Slack context, got {other:?}"),
        }
    }

    #[test]
    fn test_extract_provider_context_none_without_team() {
        let raw = json!({ "authed_user": { "id": "U1" } });
        assert!(provider().extract_provider_context(&raw).is_none());
    }

    #[test]
    fn test_parse_token_response_reads_authed_user_access_token() {
        let body = r#"{
            "ok": true,
            "app_id": "A1",
            "authed_user": {
                "id": "U1",
                "scope": "channels:read,chat:write",
                "access_token": "xoxp-user-token",
                "token_type": "user"
            },
            "team": { "id": "T1", "name": "team" }
        }"#;
        let response = provider().parse_token_response(body).unwrap();
        assert_eq!(
            response.access_token.expose_secret().as_str(),
            "xoxp-user-token"
        );
        assert!(response.refresh_token.is_none());
    }

    #[test]
    fn test_sanitize_raw_response_strips_tokens_keeps_identity() {
        let raw = json!({
            "ok": true,
            "access_token": "xoxb-bot-token",
            "refresh_token": "xoxe-bot-refresh",
            "authed_user": {
                "id": "U1",
                "scope": "channels:read",
                "access_token": "xoxp-user-token",
                "refresh_token": "xoxe-user-refresh",
            },
            "team": { "id": "T1", "name": "team" }
        });
        let cleaned = provider().sanitize_raw_response(&raw);
        assert!(cleaned.get("access_token").is_none());
        assert!(cleaned.get("refresh_token").is_none());
        let authed = cleaned.get("authed_user").unwrap();
        assert_eq!(authed.get("id").and_then(|v| v.as_str()), Some("U1"));
        assert_eq!(
            authed.get("scope").and_then(|v| v.as_str()),
            Some("channels:read")
        );
        assert!(authed.get("access_token").is_none());
        assert!(authed.get("refresh_token").is_none());
        assert_eq!(
            cleaned["team"]["id"].as_str(),
            Some("T1"),
            "team.id must be preserved"
        );
    }

    #[test]
    fn test_parse_token_response_errors_when_not_ok() {
        let body = r#"{ "ok": false, "error": "invalid_code" }"#;
        assert!(provider().parse_token_response(body).is_err());
    }
}
