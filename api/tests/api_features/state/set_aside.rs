use chrono::{DateTime, Utc};
use universal_inbox::notification::Notification;

use crate::steps::{provider::ProviderName, set_aside_support::ProviderCase};

#[derive(Default)]
pub struct SetAside {
    pub case: Option<ProviderCase>,
    pub triaged: Option<Notification>,
    pub set_aside_at: Option<DateTime<Utc>>,
}

impl SetAside {
    pub fn case_of(&self, provider: ProviderName) -> &ProviderCase {
        let case = self
            .case
            .as_ref()
            .expect("No notification was seeded in this scenario");
        assert_eq!(
            case.kind, provider.0,
            "The scenario seeded a {:?} notification, not a {:?} one",
            case.kind, provider.0
        );
        case
    }

    pub fn triaged(&self) -> &Notification {
        self.triaged
            .as_ref()
            .expect("No notification was triaged in this scenario")
    }
}
