use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum Exchange {
    Issued,
    Refused(String),
}

#[derive(Default)]
pub struct OauthRevocation {
    pub clients: HashMap<String, String>,
    pub codes: HashMap<String, String>,
    pub others_codes: HashMap<String, String>,
    pub exchange: Option<Exchange>,
    pub refresh_tokens: HashMap<String, String>,
    pub raced_clients: Vec<String>,
    pub exchange_won: usize,
}
