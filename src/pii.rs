use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use validator::ValidateLength;

/// Personal data (an email address, a name, ...) that must not reach logs or
/// telemetry by accident.
///
/// `Debug` prints a redacted placeholder and there is deliberately no
/// `Display` (nor `Deref`): formatting the wrapped value requires an explicit
/// [`Pii::expose`] call, which is easy to grep for and to review.
///
/// Serde is transparent so the wire format (API JSON, CLI, stored JSON) is the
/// one of the wrapped value. This also means serializing a `Pii` exposes it:
/// never serialize a value holding a `Pii` into a log line.
///
/// The exporter-side regex redaction (`api/src/observability/redaction.rs`)
/// stays as a safety net for what this type cannot cover.
///
/// `just check-pii-logging` rejects `expose()` / `into_inner()` written inside
/// a tracing macro or `#[tracing::instrument]` attribute. It cannot see a value
/// exposed into a variable first and logged later: don't do that either.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pii<T>(T);

impl<T> Pii<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Access the wrapped personal data. Never pass the result to a log or a
    /// tracing span.
    pub fn expose(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Pii<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pii([redacted])")
    }
}

impl<T> From<T> for Pii<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

/// Lets `#[validate(length(...))]` check a wrapped string, e.g. a user name.
impl ValidateLength<u64> for Pii<String> {
    fn length(&self) -> Option<u64> {
        self.0.length()
    }
}

impl<T: FromStr> FromStr for Pii<T> {
    type Err = T::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

#[cfg(test)]
mod tests {
    use email_address::EmailAddress;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::user::User;

    fn email() -> Pii<EmailAddress> {
        "john.doe@example.com".parse().unwrap()
    }

    #[test]
    fn test_debug_is_redacted() {
        assert_eq!(format!("{:?}", email()), "Pii([redacted])");
    }

    #[test]
    fn test_user_debug_does_not_leak_email_or_names() {
        let user = User::new(
            Some(Pii::new("Johnathan".to_string())),
            Some(Pii::new("Doeson".to_string())),
            email(),
        );

        for debug in [format!("{user:?}"), format!("{user:#?}")] {
            assert!(!debug.contains('@'));
            assert!(!debug.contains("Johnathan"));
            assert!(!debug.contains("Doeson"));
        }
    }

    #[test]
    fn test_length_validation_sees_wrapped_string() {
        assert_eq!(Pii::new("été".to_string()).length(), Some(3));
    }

    #[test]
    fn test_serde_is_transparent() {
        let json = serde_json::to_string(&email()).unwrap();
        assert_eq!(json, "\"john.doe@example.com\"");

        let parsed: Pii<EmailAddress> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, email());
    }

    #[test]
    fn test_from_str_validates_wrapped_type() {
        assert_eq!(email().expose().as_str(), "john.doe@example.com");
        assert!("not-an-email".parse::<Pii<EmailAddress>>().is_err());
    }
}
