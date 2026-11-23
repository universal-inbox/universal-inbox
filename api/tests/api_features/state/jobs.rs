use std::collections::HashMap;

use chrono::{DateTime, Utc};

use universal_inbox::user::UserId;
use universal_inbox_api::configuration::VacuumJobsCronSettings;
use universal_inbox_api::jobs::{
    JobStorage,
    dead_letter::{DeadJob, RequeueOutcome},
    metrics::QueueSample,
};

#[derive(Default)]
pub struct Jobs {
    pub storage: Option<JobStorage>,
    pub other_storage: Option<JobStorage>,
    pub vacuum_settings: Option<VacuumJobsCronSettings>,
    pub lock_results: Vec<bool>,
    pub reference_time: Option<DateTime<Utc>>,
    pub user_id: Option<UserId>,
    pub enqueued_at: Option<DateTime<Utc>>,
    pub ids: HashMap<String, String>,
    pub sample: Option<QueueSample>,
    pub listed: Vec<DeadJob>,
    pub selected: Vec<DeadJob>,
    pub requeue_outcome: Option<RequeueOutcome>,
}

impl Jobs {
    pub fn id(&self, name: &str) -> &str {
        self.ids
            .get(name)
            .unwrap_or_else(|| panic!("No job is named {name:?}"))
    }

    pub fn reference_time(&mut self) -> DateTime<Utc> {
        *self
            .reference_time
            .get_or_insert_with(|| DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap())
    }
}
