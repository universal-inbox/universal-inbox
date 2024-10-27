use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum::Display;
use url::Url;

use integration_connection::{provider::IntegrationProviderKind, NangoProviderKey, NangoPublicKey};
use utils::base64::{decode_base64, encode_base64};

#[macro_use]
extern crate macro_attr;

#[macro_use]
extern crate enum_derive;

pub mod auth;
pub mod integration_connection;
pub mod notification;
pub mod task;
pub mod third_party;
pub mod user;
pub mod utils;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct FrontConfig {
    pub authentication_configs: Vec<FrontAuthenticationConfig>,
    pub nango_base_url: Url,
    pub nango_public_key: NangoPublicKey,
    pub integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderStaticConfig>,
    pub support_href: Option<String>,
    pub show_changelog: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct IntegrationProviderStaticConfig {
    pub name: String,
    pub nango_config_key: NangoProviderKey,
    pub warning_message: Option<String>,
    pub is_enabled: bool,
    pub oauth_user_scopes: Vec<String>,
    pub required_oauth_scopes: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum FrontAuthenticationConfig {
    OIDCAuthorizationCodePKCEFlow(FrontOIDCAuthorizationCodePKCEFlowConfig),
    OIDCGoogleAuthorizationCodeFlow(FrontOIDCGoogleAuthorizationCodeFlowConfig),
    Local,
    Passkey,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct FrontOIDCAuthorizationCodePKCEFlowConfig {
    pub oidc_issuer_url: Url,
    pub oidc_client_id: String,
    pub oidc_redirect_url: Url,
    pub user_profile_url: Url,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct FrontOIDCGoogleAuthorizationCodeFlowConfig {
    pub user_profile_url: Url,
}

pub trait HasHtmlUrl {
    fn get_html_url(&self) -> Url;
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

pub const DEFAULT_PAGE_SIZE: usize = 25;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(bound = "T: Serialize + for<'d> Deserialize<'d>")]
pub struct Page<T> {
    pub per_page: usize,
    pub pages_count: usize,
    pub total: usize,
    pub previous_page_token: Option<PageToken>,
    pub next_page_token: Option<PageToken>,
    pub content: Vec<T>,
}

impl<T> Default for Page<T>
where
    T: Serialize + for<'d> Deserialize<'d>,
{
    fn default() -> Self {
        Self {
            per_page: 0,
            pages_count: 1,
            total: 0,
            previous_page_token: None,
            next_page_token: None,
            content: vec![],
        }
    }
}

impl<T> Page<T>
where
    T: Serialize + for<'d> Deserialize<'d>,
{
    pub fn remove_element<F>(&mut self, filter: F)
    where
        F: FnMut(&T) -> bool,
    {
        let original_len = self.content.len();
        self.content.retain(filter);
        if original_len != self.content.len() {
            self.total -= 1;
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash, Display)]
pub enum PageToken {
    Before(DateTime<Utc>),
    After(DateTime<Utc>),
    Offset(usize),
}

impl Default for PageToken {
    fn default() -> Self {
        PageToken::Offset(0)
    }
}

impl PageToken {
    pub fn to_url_parameter(&self) -> Result<String> {
        let json = serde_json::to_string(self)?;
        Ok(encode_base64(&json))
    }

    pub fn from_url_parameter(data: &str) -> Result<Self> {
        let decoded = decode_base64(data)?;
        Ok(serde_json::from_str(&decoded)?)
    }
}

#[cfg(test)]
pub mod test_helpers {
    use std::{env, fs};

    pub fn fixture_path(fixture_file_name: &str) -> String {
        format!(
            "{}/tests/fixtures/{fixture_file_name}",
            env::var("CARGO_MANIFEST_DIR").unwrap()
        )
    }
    pub fn load_json_fixture_file<T: for<'de> serde::de::Deserialize<'de>>(
        fixture_file_name: &str,
    ) -> T {
        let input_str = fs::read_to_string(fixture_path(fixture_file_name)).unwrap();
        serde_json::from_str::<T>(&input_str).unwrap()
    }
}
