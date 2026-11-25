use reqwest::Client;
use serde_json::Value;

#[derive(Default)]
pub struct McpOauth {
    pub client: Option<Value>,
    pub browser: Option<Client>,
    pub verifier: Option<String>,
    pub code: Option<String>,
    pub request_id: Option<String>,
    pub csrf_token: Option<String>,
    pub decision_redirect: Option<String>,
    pub tokens: Option<Value>,
    pub first_refresh_token: Option<String>,
    pub previous_access_token: Option<String>,
    pub refused: Vec<Value>,
}

impl McpOauth {
    pub fn client_id(&self) -> &str {
        self.client
            .as_ref()
            .expect("No OAuth2 client is registered")["client_id"]
            .as_str()
            .expect("The OAuth2 client has no client_id")
    }

    pub fn token(&self, name: &str) -> &str {
        self.tokens.as_ref().expect("No OAuth2 token was issued")[name]
            .as_str()
            .unwrap_or_else(|| panic!("The token response has no {name}"))
    }

    pub fn verifier(&self) -> &str {
        self.verifier
            .as_deref()
            .expect("The scenario gave no PKCE verifier")
    }
}
