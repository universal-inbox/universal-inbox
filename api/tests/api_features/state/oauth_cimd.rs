use reqwest::Client;

use crate::common::LazyMockServer;

#[derive(Default)]
pub struct OauthCimd {
    pub server: Option<LazyMockServer>,
    pub others_browser: Option<Client>,
    pub others_request_id: Option<String>,
}
