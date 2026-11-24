use std::str::FromStr;

use cucumber::Parameter;

use universal_inbox::notification::NotificationStatus;

#[derive(Clone, Copy, Debug)]
pub struct UserStatus(pub NotificationStatus);

impl Parameter for UserStatus {
    const REGEX: &'static str = "unread|read|deleted|unsubscribed";
    const NAME: &'static str = "status";
}

impl FromStr for UserStatus {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(Self(match name {
            "unread" => NotificationStatus::Unread,
            "read" => NotificationStatus::Read,
            "deleted" => NotificationStatus::Deleted,
            "unsubscribed" => NotificationStatus::Unsubscribed,
            other => return Err(format!("Unknown status {other:?}")),
        }))
    }
}
