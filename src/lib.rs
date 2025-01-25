use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

use integration_connection::{provider::IntegrationProviderKind, NangoProviderKey, NangoPublicKey};
use user::UserAuth;

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
    pub doc: String,
    pub warning_message: Option<String>,
    pub doc_for_actions: HashMap<String, String>,
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

impl FrontAuthenticationConfig {
    pub fn match_user_auth(&self, user_auth: &UserAuth) -> bool {
        match self {
            Self::OIDCAuthorizationCodePKCEFlow(_) => {
                matches!(user_auth, UserAuth::OIDCAuthorizationCodePKCE(_))
            }
            Self::OIDCGoogleAuthorizationCodeFlow(_) => {
                matches!(user_auth, UserAuth::OIDCGoogleAuthorizationCode(_))
            }
            Self::Local => matches!(user_auth, UserAuth::Local(_)),
        }
    }
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

impl<T> Page<T> {
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
