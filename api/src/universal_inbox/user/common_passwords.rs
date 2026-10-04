//! Common-password check applied to every new password (ASVS 2.1.7).
//!
//! The list is embedded in the API binary (not the web bundle) and only holds
//! entries long enough to pass the length policy: shorter ones are already
//! rejected by `Password::check_policy`.

use std::{collections::HashSet, sync::LazyLock};

static COMMON_PASSWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    include_str!("common_passwords.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
});

/// Whether `password` appears in the common-password list, ignoring case.
pub fn is_common_password(password: &str) -> bool {
    COMMON_PASSWORDS.contains(password.to_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listed_password_is_common_whatever_its_case() {
        assert!(is_common_password("1qaz2wsx3edc"));
        assert!(is_common_password("1QAZ2wsx3EDC"));
    }

    #[test]
    fn an_unlisted_password_is_not_common() {
        assert!(!is_common_password("Very-harD-pasSword-5"));
    }

    #[test]
    fn comment_lines_are_not_passwords() {
        assert!(
            !COMMON_PASSWORDS
                .iter()
                .any(|password| password.starts_with('#'))
        );
    }
}
