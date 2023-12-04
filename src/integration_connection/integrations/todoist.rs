use std::{default::Default, fmt};

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistContext {
    pub items_sync_token: SyncToken,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct SyncToken(pub String);

impl fmt::Display for SyncToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<SyncToken> for String {
    fn from(sync_token: SyncToken) -> Self {
        sync_token.0
    }
}

impl From<String> for SyncToken {
    fn from(sync_token: String) -> Self {
        Self(sync_token)
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone, Default)]
pub struct TodoistConfig {
    pub sync_tasks_enabled: bool,
}

impl TodoistConfig {
    pub fn enabled() -> Self {
        Self {
            sync_tasks_enabled: true,
        }
    }
}
