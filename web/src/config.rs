use anyhow::Result;
use fermi::AtomRef;
use reqwest::Method;
use url::Url;
use wasm_bindgen::prelude::*;

use universal_inbox::FrontConfig;

use crate::services::api::call_api;

#[derive(Debug, PartialEq)]
pub struct AppConfig {
    pub api_base_url: Url,
    pub oidc_issuer_url: Url,
    pub oidc_client_id: String,
    pub oidc_redirect_url: Url,
}

#[wasm_bindgen(module = "/js/api.js")]
extern "C" {
    fn api_base_url() -> String;
}

pub static APP_CONFIG: AtomRef<Option<AppConfig>> = |_| None;

pub fn get_api_base_url() -> Result<Url> {
    Ok(Url::parse(&api_base_url())?)
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
    };
    Ok(app_config)
}
