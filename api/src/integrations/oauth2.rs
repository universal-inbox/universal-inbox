use anyhow::{anyhow, Context};
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use format_serde_error::SerdeError;
use http::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use universal_inbox::integration_connection::{ConnectionId, NangoProviderKey};
use url::Url;

use crate::{integrations::APP_USER_AGENT, universal_inbox::UniversalInboxError};

#[derive(Clone, Debug)]
pub struct NangoService {
    client: reqwest::Client,
    nango_base_url: Url,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct NangoConnection {
    pub id: u32,
    pub account_id: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub provider_config_key: NangoProviderKey,
    pub connection_id: ConnectionId,
    pub credentials: NangoConnectionCredentials,
    pub connection_config: Value,
    pub metadata: Value,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct NangoConnectionCredentials {
    pub r#type: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub raw: Value,
}

impl NangoService {
    pub fn new(nango_base_url: Url, secret_key: &str) -> Result<NangoService, UniversalInboxError> {
        Ok(NangoService {
            client: build_nango_client(secret_key).context("Cannot build Nango client")?,
            nango_base_url,
        })
    }

    #[tracing::instrument(level = "debug", skip(self), ret, err)]
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
            return Ok(None);
        };

        let response_body = response
            .text()
            .await
            .context(format!("Failed to fetch connection response for {connection_id} for provider {provider_config_key} from Nango API"))?;

        let connection: NangoConnection = serde_json::from_str(&response_body)
            .map_err(|err| SerdeError::new(response_body, err))
            .context("Failed to parse response")?;

        Ok(Some(connection))
    }

    #[tracing::instrument(level = "debug", skip(self), ret, err)]
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
        if status_code != reqwest::StatusCode::BAD_REQUEST && status_code != reqwest::StatusCode::OK
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

fn build_nango_client(secret_key: &str) -> Result<reqwest::Client, reqwest::Error> {
    let mut headers = HeaderMap::new();

    let base64_secret_key = general_purpose::STANDARD.encode(secret_key);
    let mut auth_header_value: HeaderValue = format!("Basic {base64_secret_key}:").parse().unwrap();
    auth_header_value.set_sensitive(true);
    headers.insert("Authorization", auth_header_value);

    reqwest::Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()
}
