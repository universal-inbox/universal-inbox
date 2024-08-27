use std::fmt::Debug;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct DefaultValue<T> {
    pub default_value: T,
    pub value: Option<T>,
}

impl<T> DefaultValue<T> {
    pub fn new(default_value: T, value: Option<T>) -> Self {
        Self {
            default_value,
            value,
        }
    }

    pub fn has_value(&self) -> bool {
        self.value.is_some()
    }

    pub fn into_value(self) -> T {
        self.value.unwrap_or(self.default_value)
    }
}
