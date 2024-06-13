use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

use integration_connection::{provider::IntegrationProviderKind, NangoProviderKey, NangoPublicKey};

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
    pub authentication_config: FrontAuthenticationConfig,
    pub nango_base_url: Url,
    pub nango_public_key: NangoPublicKey,
    pub integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderStaticConfig>,
    pub support_href: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct IntegrationProviderStaticConfig {
    pub name: String,
    pub nango_config_key: NangoProviderKey,
    pub doc: String,
    pub warning_message: Option<String>,
    pub doc_for_actions: HashMap<String, String>,
    pub is_implemented: bool,
    pub oauth_user_scopes: Vec<String>,
    pub required_oauth_scopes: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum FrontAuthenticationConfig {
    OIDCAuthorizationCodePKCEFlow {
        oidc_issuer_url: Url,
        oidc_client_id: String,
        oidc_redirect_url: Url,
        user_profile_url: Url,
    },
    OIDCGoogleAuthorizationCodeFlow {
        user_profile_url: Url,
    },
    Local,
}

pub trait HasHtmlUrl {
    fn get_html_url(&self) -> Url;
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash, Default)]
pub struct Page<T> {
    pub page: usize,
    pub per_page: usize,
    pub total: usize,
    pub content: Vec<T>,
}
