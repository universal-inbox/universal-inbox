use serde_json::Value;

#[derive(Default)]
pub struct McpOauth {
    pub client: Option<Value>,
}

impl McpOauth {
    pub fn client_id(&self) -> &str {
        self.client
            .as_ref()
            .expect("No OAuth2 client is registered")["client_id"]
            .as_str()
            .expect("The OAuth2 client has no client_id")
    }
}
