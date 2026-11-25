use std::collections::HashMap;

#[derive(Default)]
pub struct OauthRevocation {
    pub clients: HashMap<String, String>,
    pub codes: HashMap<String, String>,
    pub exchange_issued_tokens: Option<bool>,
    pub raced_clients: Vec<String>,
}
