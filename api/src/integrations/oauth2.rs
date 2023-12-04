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

    #[tracing::instrument(level = "debug", skip(self), err)]
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

    #[tracing::instrument(level = "debug", skip(self), err)]
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
