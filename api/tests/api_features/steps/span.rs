use std::str::FromStr;

use chrono::TimeDelta;
use cucumber::Parameter;

#[derive(Clone, Copy, Debug)]
pub struct Span(pub TimeDelta);

impl Parameter for Span {
    const REGEX: &'static str = r"\d+ days?(?: and \d+ hours?)?";
    const NAME: &'static str = "duration";
}

impl FromStr for Span {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts = text.split(" and ");
        let count = |part: &str| -> Result<i64, String> {
            part.split(' ')
                .next()
                .and_then(|count| count.parse().ok())
                .ok_or_else(|| format!("Unknown duration {text:?}"))
        };
        let days = count(parts.next().unwrap_or_default())?;
        let hours = parts.next().map(count).transpose()?.unwrap_or(0);
        Ok(Self(TimeDelta::days(days) + TimeDelta::hours(hours)))
    }
}
