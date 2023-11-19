use std::collections::HashMap;

use anyhow::Result;
use fermi::AtomRef;
use reqwest::Method;
use url::Url;
use wasm_bindgen::prelude::*;

use universal_inbox::{
    integration_connection::{IntegrationProviderKind, NangoPublicKey},
    FrontConfig, IntegrationProviderConfig,
};

use crate::{services::api::call_api, utils::current_origin};

#[derive(Debug, PartialEq)]
pub struct AppConfig {
    pub api_base_url: Url,
    pub oidc_issuer_url: Url,
    pub oidc_client_id: Option<String>,
    pub oidc_redirect_url: Url,
    pub user_profile_url: Url,
    pub nango_base_url: Url,
    pub nango_public_key: NangoPublicKey,
    pub integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderConfig>,
    pub support_href: Option<String>,
}

#[wasm_bindgen(module = "/js/api.js")]
extern "C" {
    fn api_base_url() -> String;
}

pub static APP_CONFIG: AtomRef<Option<AppConfig>> = AtomRef(|_| None);

pub fn get_api_base_url() -> Result<Url> {
    match Url::parse(&api_base_url()) {
        Ok(url) => Ok(url),
        Err(err) => match current_origin()?.join(&api_base_url()) {
            Ok(url) => Ok(url),
            Err(_) => Err(anyhow::anyhow!("Failed to parse api_base_url: {}", err)),
        },
    }
}

pub async fn get_app_config() -> Result<AppConfig> {
    let api_base_url = get_api_base_url()?;
    let front_config: FrontConfig = call_api(
        Method::GET,
        &api_base_url,
        "front_config",
        None::<i32>,
        None,
    )
    .await?;

    let app_config = AppConfig {
        api_base_url,
        oidc_issuer_url: front_config.oidc_issuer_url,
        oidc_client_id: front_config.oidc_client_id,
        oidc_redirect_url: front_config.oidc_redirect_url,
        user_profile_url: front_config.user_profile_url,
        nango_base_url: front_config.nango_base_url,
        nango_public_key: front_config.nango_public_key,
        integration_providers: front_config.integration_providers,
        support_href: front_config.support_href,
    };
    Ok(app_config)
}
