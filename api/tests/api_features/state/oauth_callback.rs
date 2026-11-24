use http::StatusCode;

use universal_inbox::integration_connection::IntegrationConnectionId;

#[derive(Default)]
pub struct OauthCallback {
    pub connection_id: Option<IntegrationConnectionId>,
    pub state: Option<String>,
    pub authorization_url: Option<url::Url>,
    pub status: Option<StatusCode>,
    pub location: Option<String>,
}
