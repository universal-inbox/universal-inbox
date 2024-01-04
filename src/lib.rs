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
pub mod user;

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
