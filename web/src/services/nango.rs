use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use universal_inbox::integration_connection::{ConnectionId, NangoProviderKey};
use url::Url;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/nango.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn auth_provider(
        nango_host: &str,
        config_key: &str,
        connection_id: &str,
    ) -> Result<JsValue, JsValue>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct NangoAuthResult {
    pub provider_config_key: NangoProviderKey,
    pub connection_id: ConnectionId,
}

pub async fn nango_auth(
    nango_base_url: &Url,
    nango_provider_key: &NangoProviderKey,
    nango_connection_id: &ConnectionId,
) -> Result<NangoAuthResult> {
    let result = auth_provider(
        nango_base_url.as_ref(),
        nango_provider_key.to_string().as_str(),
        nango_connection_id.to_string().as_str(),
    )
    .await
    .map_err(|err| anyhow!("Failed to authorize integration: {:?}", err))?;

    serde_wasm_bindgen::from_value(result).map_err(|err| {
        anyhow!(
            "Failed to retrieve result while authorizing integration: {:?}",
            err
        )
    })
}