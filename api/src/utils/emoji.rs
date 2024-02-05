pub fn replace_emoji_code_in_string_with_emoji(string: &str) -> String {
    let mut result = String::new();
    let mut chars = string.chars();
    #[allow(clippy::while_let_on_iterator)]
    while let Some(c) = chars.next() {
        if c == ':' {
            let mut emoji_code = String::new();
            while let Some(c) = chars.next() {
                if c == ':' {
                    break;
                }
                emoji_code.push(c);
            }
            if let Some(emoji) = emojis::get_by_shortcode(&emoji_code.to_lowercase()) {
                result.push_str(emoji.as_str());
            } else {
                result.push(':');
                result.push_str(&emoji_code);
                result.push(':');
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn replace_emoji_code_with_emoji(string: &str) -> Option<String> {
    let replace_from = if string.starts_with(':') && string.ends_with(':') {
        &string[1..string.len() - 1]
    } else {
        string
    };
    emojis::get_by_shortcode(&replace_from.to_lowercase()).map(|emoji| emoji.to_string())
}

#[cfg(test)]
mod tests {
    mod replace_emoji_code_in_string_with_emoji {
        use super::super::*;
        use pretty_assertions::assert_eq;
        use rstest::*;

        #[rstest]
        fn test_replace_no_emoji() {
            assert_eq!(
                replace_emoji_code_in_string_with_emoji("Hello World"),
                "Hello World".to_string()
            );
        }

        #[rstest]
        fn test_replace_unknown_emoji() {
            assert_eq!(
                replace_emoji_code_in_string_with_emoji("Hello World :Unknown_emoji:!"),
                "Hello World :Unknown_emoji:!".to_string()
            );
        }

        #[rstest]
        fn test_replace_known_emoji() {
            assert_eq!(
                replace_emoji_code_in_string_with_emoji("Hello World :rocket:!"),
                "Hello World 🚀!".to_string()
            );
        }

        #[rstest]
        fn test_replace_known_emoji_random_case() {
            assert_eq!(
                replace_emoji_code_in_string_with_emoji("Hello World :RoCkEt:!"),
                "Hello World 🚀!".to_string()
            );
        }
    }

    mod replace_emoji_code_with_emoji {
        use super::super::*;
        use pretty_assertions::assert_eq;
        use rstest::*;

        #[rstest]
        fn test_replace_unknown_emoji() {
            assert!(replace_emoji_code_with_emoji("Noemoji").is_none(),);
        }

        #[rstest]
        fn test_replace_known_emoji() {
            assert_eq!(
                replace_emoji_code_with_emoji("rocket"),
                Some("🚀".to_string())
            );
        }

        #[rstest]
        fn test_replace_known_emoji_random_case() {
            assert_eq!(
                replace_emoji_code_with_emoji("RoCkEt"),
                Some("🚀".to_string())
            );
        }

        #[rstest]
        fn test_replace_unknown_emoji_with_suffix_and_prefix() {
            assert!(replace_emoji_code_with_emoji(":Noemoji:").is_none(),);
        }

        #[rstest]
        fn test_replace_known_emoji_with_suffix_and_prefix() {
            assert_eq!(
                replace_emoji_code_with_emoji(":rocket:"),
                Some("🚀".to_string())
            );
        }

        #[rstest]
        fn test_replace_known_emoji_random_case_with_suffix_and_prefix() {
            assert_eq!(
                replace_emoji_code_with_emoji(":RoCkEt:"),
                Some("🚀".to_string())
            );
        }
    }
}
