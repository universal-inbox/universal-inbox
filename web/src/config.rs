use std::collections::HashMap;

use anyhow::Result;
use dioxus::prelude::*;
use reqwest::Method;
use url::Url;

use universal_inbox::{
    integration_connection::{provider::IntegrationProviderKind, NangoPublicKey},
    FrontAuthenticationConfig, FrontConfig, IntegrationProviderStaticConfig,
};

use crate::{services::api::call_api, utils::current_origin};

#[derive(Debug, PartialEq, Clone)]
pub struct AppConfig {
    pub authentication_configs: Vec<FrontAuthenticationConfig>,
    pub api_base_url: Url,
    pub nango_base_url: Url,
    pub nango_public_key: NangoPublicKey,
    pub integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderStaticConfig>,
    pub support_href: Option<String>,
    pub show_changelog: bool,
}

pub static APP_CONFIG: GlobalSignal<Option<AppConfig>> = Signal::global(|| None);

pub fn get_api_base_url() -> Result<Url> {
    match current_origin()?.join("/api/") {
        Ok(url) => Ok(url),
        Err(err) => Err(anyhow::anyhow!("Failed to parse api_base_url: {}", err)),
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
        authentication_configs: front_config.authentication_configs,
        nango_base_url: front_config.nango_base_url,
        nango_public_key: front_config.nango_public_key,
        integration_providers: front_config.integration_providers,
        support_href: front_config.support_href,
        show_changelog: front_config.show_changelog,
    };
    Ok(app_config)
}
