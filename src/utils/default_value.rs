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

    /// Resolve the effective value against an already stored one.
    ///
    /// Unlike [`DefaultValue::into_value`], which falls back to the
    /// `default_value` (the seed used when there is nothing stored yet), this
    /// falls back to `fallback` — the value currently persisted. It is what the
    /// repository uses when an integration declined to own a field: the stored
    /// value wins and the column is left untouched.
    pub fn value_or(self, fallback: T) -> T {
        self.value.unwrap_or(fallback)
    }
}
