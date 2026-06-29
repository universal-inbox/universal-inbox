use chrono::NaiveTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Time-of-day configuration applied on top of a task's due *date*.
///
/// When present on a task creation/planning request (or as an integration's
/// default), the bare due date is upgraded to a timezone-aware due datetime:
/// the date is combined with [`time`](Self::time), interpreted in
/// [`timezone`](Self::timezone), then converted to UTC
/// ([`crate::task::DueDate::DateTimeWithTz`]).
///
/// `timezone` is always set — the frontend popover defaults it to the browser
/// timezone — so a time is never ambiguous.
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone, JsonSchema)]
pub struct TaskTimeConfig {
    /// Local time-of-day (HH:MM) applied to the due date.
    pub time: NaiveTime,
    /// Optional task duration, in minutes.
    pub duration_minutes: Option<u32>,
    /// IANA timezone name (e.g. `"Europe/Paris"`) the `time` is expressed in.
    pub timezone: String,
}
