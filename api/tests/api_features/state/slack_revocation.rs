use std::collections::HashMap;

use universal_inbox::integration_connection::IntegrationConnectionId;

#[derive(Default)]
pub struct SlackRevocation {
    pub connections: HashMap<String, IntegrationConnectionId>,
    pub status: Option<u16>,
}
