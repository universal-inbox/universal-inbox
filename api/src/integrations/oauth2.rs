use std::fmt;

use anyhow::{anyhow, Context};
use chrono::{DateTime, Utc};
use http::{HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{SpanBackendWithUrl, TracingMiddleware};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use tracing::warn;
use universal_inbox::integration_connection::{ConnectionId, NangoProviderKey};
use url::Url;

use crate::{integrations::APP_USER_AGENT, universal_inbox::UniversalInboxError};

#[derive(Clone, Debug)]
pub struct NangoService {
    client: ClientWithMiddleware,
    nango_base_url: Url,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct NangoConnection {
    pub id: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub deleted: bool,
    pub environment_id: u32,
    pub last_fetched_at: Option<DateTime<Utc>>,
    pub provider_config_key: NangoProviderKey,
    pub connection_id: ConnectionId,
    pub credentials: NangoConnectionCredentials,
    pub connection_config: Value,
    pub metadata: Value,
    pub credentials_iv: Value,
    pub credentials_tag: Value,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct NangoConnectionCredentials {
    pub r#type: String,
    pub access_token: AccessToken,
    pub refresh_token: Option<RefreshToken>,
    pub expires_at: Option<DateTime<Utc>>,
    pub raw: Value,
}

impl NangoConnection {
    pub fn get_provider_user_id(&self) -> Option<String> {
        match self.provider_config_key.0.as_str() {
            "slack" => Some(
                self.credentials.raw["authed_user"]["id"]
                    .as_str()?
                    .to_string(),
            ),
            _ => None,
        }
    }

    pub fn get_registered_oauth_scopes(&self) -> Vec<String> {
        match self.provider_config_key.0.as_str() {
            "slack" => self.credentials.raw["authed_user"]["scope"]
                .as_str()
                .unwrap_or_default()
                .split(',')
                .map(|scope| scope.to_string())
                .collect(),
            // Todoist scopes are not stored in the connection raw credentials
            "todoist" => vec![],
            "google-mail" => {
                if let Some(scope) = self.credentials.raw["scope"].as_str() {
                    vec![scope.to_string()]
                } else {
                    vec![]
                }
            }
            "linear" => self.credentials.raw["scope"]
                .as_array()
                .map(|scopes| {
                    scopes
                        .iter()
                        .filter_map(|scope| Some(scope.as_str()?.to_string()))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
            "github" => self.credentials.raw["scope"]
                .as_str()
                .unwrap_or_default()
                .split(',')
                .map(|scope| scope.to_string())
                .collect(),
            _ => vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash, Default)]
#[serde(transparent)]
pub struct AccessToken(pub String);

impl fmt::Display for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct RefreshToken(pub String);

impl fmt::Display for RefreshToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl NangoService {
    pub fn new(nango_base_url: Url, secret_key: &str) -> Result<NangoService, UniversalInboxError> {
        Ok(NangoService {
            client: build_nango_client(secret_key).context("Cannot build Nango client")?,
            nango_base_url,
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_connection(
        &self,
        connection_id: ConnectionId,
        provider_config_key: &NangoProviderKey,
    ) -> Result<Option<NangoConnection>, UniversalInboxError> {
        let response = self
            .client
            .get(&format!(
                "{}connection/{connection_id}?provider_config_key={provider_config_key}",
                self.nango_base_url
            ))
             .send()
            .await
            .context(format!("Cannot fetch connection {connection_id} for provider {provider_config_key} from Nango API"))?;

        if response.status() == reqwest::StatusCode::BAD_REQUEST {
            warn!(
                "Nango API returned 400 Bad Request: {}",
                response
                    .text()
                    .await
                    .context("Failed to fetch connection response for {connection_id} for provider {provider_config_key} from Nango API")?
            );
            return Ok(None);
        };

        let response_body = response
            .text()
            .await
            .context(format!("Failed to fetch connection response for {connection_id} for provider {provider_config_key} from Nango API"))?;

        let connection: NangoConnection = serde_json::from_str(&response_body)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response_body))?;

        Ok(Some(connection))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn delete_connection(
        &self,
        connection_id: ConnectionId,
        provider_config_key: &NangoProviderKey,
    ) -> Result<(), UniversalInboxError> {
        let response = self
            .client
            .delete(&format!(
                "{}connection/{connection_id}?provider_config_key={provider_config_key}",
                self.nango_base_url
            ))
             .send()
            .await
            .context(format!("Cannot fetch connection {connection_id} for provider {provider_config_key} from Nango API"))?;

        let status_code = response.status();
        // We consider the connection already deleted even when receiving a BAD_REQUEST response
        if status_code != reqwest::StatusCode::BAD_REQUEST
            && status_code != reqwest::StatusCode::NO_CONTENT
        {
            return Err(
                UniversalInboxError::Unexpected(
                    anyhow!(
                        "Failed to delete connection {connection_id} for provider {provider_config_key} from Nango API: unexpected response status code {status_code}"
                    )
                )
            );
        };

        Ok(())
    }
}

fn build_nango_client(secret_key: &str) -> Result<ClientWithMiddleware, reqwest::Error> {
    let mut headers = HeaderMap::new();

    let mut auth_header_value: HeaderValue = format!("Bearer {secret_key}").parse().unwrap();
    auth_header_value.set_sensitive(true);
    headers.insert("Authorization", auth_header_value);

    let reqwest_client = reqwest::Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()?;
    Ok(ClientBuilder::new(reqwest_client)
        .with(TracingMiddleware::<SpanBackendWithUrl>::new())
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use rstest::*;

    mod get_registered_oauth_scopes {
        use pretty_assertions::assert_eq;
        use serde_json::json;
        use uuid::Uuid;

        use super::*;

        #[fixture]
        fn connection() -> NangoConnection {
            NangoConnection {
                id: 1,
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                deleted_at: None,
                deleted: false,
                environment_id: 1,
                last_fetched_at: None,
                provider_config_key: NangoProviderKey("slack".to_string()),
                connection_id: ConnectionId(Uuid::new_v4()),
                credentials: NangoConnectionCredentials {
                    r#type: "oauth".to_string(),
                    access_token: AccessToken("access_token".to_string()),
                    refresh_token: Some(RefreshToken("refresh_token".to_string())),
                    expires_at: None,
                    raw: json!({
                        "authed_user": {
                            "id": "U123456",
                            "scope": "channels:read,chat:write,users:read"
                        }
                    }),
                },
                connection_config: json!({}),
                metadata: json!({}),
                credentials_iv: json!({}),
                credentials_tag: json!({}),
            }
        }

        #[rstest]
        fn test_slack_scopes_extractions(mut connection: NangoConnection) {
            connection.provider_config_key = NangoProviderKey("slack".to_string());
            connection.credentials.raw = serde_json::json!({
                "authed_user": {
                    "scope": "channels:read,chat:write,users:read"
                }
            });

            let scopes = connection.get_registered_oauth_scopes();

            assert_eq!(scopes.len(), 3);
            assert!(scopes.contains(&"channels:read".to_string()));
            assert!(scopes.contains(&"chat:write".to_string()));
            assert!(scopes.contains(&"users:read".to_string()));
        }

        #[rstest]
        fn test_todoist_scopes_extractions(mut connection: NangoConnection) {
            connection.provider_config_key = NangoProviderKey("todoist".to_string());
            // Todoist scopes are not stored in the connection raw credentials
            connection.credentials.raw = serde_json::json!({});

            let scopes = connection.get_registered_oauth_scopes();

            assert_eq!(scopes.len(), 0);
        }

        #[rstest]
        fn test_google_mail_scopes_extractions(mut connection: NangoConnection) {
            connection.provider_config_key = NangoProviderKey("google-mail".to_string());
            connection.credentials.raw = serde_json::json!({
                "scope": "https://www.googleapis.com/auth/gmail.readonly"
            });

            let scopes = connection.get_registered_oauth_scopes();

            assert_eq!(scopes.len(), 1);
            assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.readonly".to_string()));
        }

        #[rstest]
        fn test_linear_scopes_extractions(mut connection: NangoConnection) {
            connection.provider_config_key = NangoProviderKey("linear".to_string());
            connection.credentials.raw = serde_json::json!({
                "scope": ["read", "write"]
            });

            let scopes = connection.get_registered_oauth_scopes();

            assert_eq!(scopes.len(), 2);
            assert!(scopes.contains(&"read".to_string()));
            assert!(scopes.contains(&"write".to_string()));
        }

        #[rstest]
        fn test_github_scopes_extractions(mut connection: NangoConnection) {
            connection.provider_config_key = NangoProviderKey("github".to_string());
            connection.credentials.raw = serde_json::json!({
                "scope": "repo,read:org"
            });

            let scopes = connection.get_registered_oauth_scopes();

            assert_eq!(scopes.len(), 2);
            assert!(scopes.contains(&"repo".to_string()));
            assert!(scopes.contains(&"read:org".to_string()));
        }
    }
}
