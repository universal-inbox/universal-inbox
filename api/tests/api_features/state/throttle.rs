//! What the scenarios of the throttling features keep between their steps.

use reqwest::StatusCode;

/// What a client can observe of a response, to compare the answers served for registered and
/// unregistered email addresses.
#[derive(Debug, PartialEq, Eq)]
pub struct Observed {
    pub status: StatusCode,
    pub has_retry_after: bool,
    pub body: String,
}

/// Requests a scenario sends from a different network address each time, to show that only the
/// email address, not the address of the sender, is throttled.
#[derive(Debug, Default)]
pub struct Probe {
    /// Index of the next network address to send from.
    pub next: u32,
    /// What each series of requests got, one series per email address.
    pub series: Vec<Vec<Observed>>,
}

impl Probe {
    /// A network address of its own for each request.
    pub fn next_address(&mut self) -> String {
        let address = format!("198.51.100.{}", self.next % 250 + 1);
        self.next += 1;
        address
    }
}
