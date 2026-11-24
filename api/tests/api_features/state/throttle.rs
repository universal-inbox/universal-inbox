use reqwest::StatusCode;

#[derive(Debug, PartialEq, Eq)]
pub struct Observed {
    pub status: StatusCode,
    pub has_retry_after: bool,
    pub body: String,
}

#[derive(Debug, Default)]
pub struct Probe {
    pub next: u32,
    pub series: Vec<Vec<Observed>>,
    pub burst: Vec<reqwest::StatusCode>,
}

impl Probe {
    pub fn next_address(&mut self) -> String {
        let address = format!("198.51.100.{}", self.next % 250 + 1);
        self.next += 1;
        address
    }
}
