use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

use serde_json::Value;

use universal_inbox::{
    task::Task,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData},
};
use universal_inbox_api::universal_inbox::data_encryption::{
    DataEncryptionProgress, DataEncryptionStatus,
};

#[derive(Default)]
pub struct RecordedProgress {
    processed: Mutex<BTreeMap<String, u64>>,
    done: Mutex<BTreeSet<String>>,
}

impl DataEncryptionProgress for RecordedProgress {
    fn batch_done(&self, column: &str, processed: u64) {
        *self
            .processed
            .lock()
            .unwrap()
            .entry(column.to_string())
            .or_default() += processed;
    }

    fn column_done(&self, column: &str) {
        self.done.lock().unwrap().insert(column.to_string());
    }
}

impl RecordedProgress {
    pub fn done(&self) -> BTreeSet<String> {
        self.done.lock().unwrap().clone()
    }

    pub fn processed(&self) -> BTreeMap<String, u64> {
        self.processed
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(column, count)| (column.clone(), *count))
            .collect()
    }
}

pub struct StoredItem {
    pub item: ThirdPartyItem,
    pub data: ThirdPartyItemData,
    pub task: Option<Task>,
}

#[derive(Default)]
pub struct Encryption {
    pub status_before: Option<DataEncryptionStatus>,
    pub status: Option<DataEncryptionStatus>,
    pub progress: RecordedProgress,
    pub plaintext_counts: Vec<(String, i64)>,
    pub stored_item: Option<StoredItem>,
    pub expected_task_body: Option<String>,
    pub raw_token_response: Option<RawTokenResponse>,
}

pub struct RawTokenResponse {
    pub given: Value,
    pub reported: Value,
}
