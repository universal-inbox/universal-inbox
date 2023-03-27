use serde::{Deserialize, Serialize};
use url::Url;

#[macro_use]
extern crate macro_attr;

#[macro_use]
extern crate enum_derive;

pub mod notification;
pub mod task;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct FrontConfig {
    pub oidc_issuer_url: Url,
    pub oidc_client_id: String,
    pub oidc_redirect_url: Url,
}
