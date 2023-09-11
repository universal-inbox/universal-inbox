use std::collections::HashMap;

use http::Uri;
use serde::{Deserialize, Serialize};
use url::Url;

use integration_connection::{IntegrationProviderKind, NangoProviderKey};

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
    pub oidc_issuer_url: Url,
    pub oidc_client_id: String,
    pub oidc_redirect_url: Url,
    pub user_profile_url: Url,
    pub nango_base_url: Url,
    pub integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderConfig>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct IntegrationProviderConfig {
    pub name: String,
    pub nango_config_key: NangoProviderKey,
    pub comment: Option<String>,
    pub is_implemented: bool,
}

pub trait HasHtmlUrl {
    fn get_html_url(&self) -> Uri;
}
