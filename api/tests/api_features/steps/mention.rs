use std::str::FromStr;

use cucumber::Parameter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mention {
    Mentioning,
    NotMentioning,
}

impl Parameter for Mention {
    const REGEX: &'static str = "mentioning me|not mentioning me";
    const NAME: &'static str = "mention";
}

impl FromStr for Mention {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "mentioning me" => Ok(Self::Mentioning),
            "not mentioning me" => Ok(Self::NotMentioning),
            other => Err(format!("Unknown mention {other:?}")),
        }
    }
}
