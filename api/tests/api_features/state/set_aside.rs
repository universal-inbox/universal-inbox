use chrono::{DateTime, Utc};
use universal_inbox::notification::Notification;

use crate::steps::set_aside_support::ProviderCase;

#[derive(Default)]
pub struct SetAside {
    pub case: Option<ProviderCase>,
    pub triaged: Option<Notification>,
    pub set_aside_at: Option<DateTime<Utc>>,
}

impl SetAside {
    pub fn case(&self) -> &ProviderCase {
        self.case
            .as_ref()
            .expect("No notification was seeded in this scenario")
    }

    pub fn triaged(&self) -> &Notification {
        self.triaged
            .as_ref()
            .expect("No notification was triaged in this scenario")
    }
}
