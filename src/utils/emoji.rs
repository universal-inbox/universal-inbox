pub fn search_emojis_by_shortcode(query: &str, limit: usize) -> Vec<(String, String)> {
    let query_lower = query.to_lowercase();

    let mut starts_with = Vec::new();
    let mut contains = Vec::new();

    for emoji in emojis::iter() {
        for shortcode in emoji.shortcodes() {
            let sc_lower = shortcode.to_lowercase();
            if sc_lower.starts_with(&query_lower) {
                starts_with.push((shortcode.to_string(), emoji.to_string()));
            } else if sc_lower.contains(&query_lower) {
                contains.push((shortcode.to_string(), emoji.to_string()));
            }
            if starts_with.len() + contains.len() >= limit * 2 {
                break;
            }
        }
    }

    starts_with.extend(contains);
    starts_with.truncate(limit);
    starts_with
}

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

    mod search_emojis_by_shortcode_tests {
        use super::super::*;
        use rstest::*;

        #[rstest]
        fn test_search_starts_with_match() {
            let results = search_emojis_by_shortcode("rock", 10);
            assert!(!results.is_empty());
            assert!(results.iter().any(|(sc, _)| sc == "rocket"));
        }

        #[rstest]
        fn test_search_case_insensitive() {
            let results = search_emojis_by_shortcode("ROCK", 10);
            assert!(results.iter().any(|(sc, _)| sc == "rocket"));
        }

        #[rstest]
        fn test_search_contains_match() {
            let results = search_emojis_by_shortcode("ocket", 10);
            assert!(results.iter().any(|(sc, _)| sc == "rocket"));
        }

        #[rstest]
        fn test_search_prioritizes_starts_with() {
            let results = search_emojis_by_shortcode("eye", 50);
            // "eyes" starts with "eye" so it should appear before any contains-only matches
            if let Some(pos) = results.iter().position(|(sc, _)| sc == "eyes") {
                // All items before "eyes" should also start with "eye"
                for (sc, _) in &results[..pos] {
                    assert!(
                        sc.to_lowercase().starts_with("eye"),
                        "{sc} should start with 'eye'"
                    );
                }
            }
        }

        #[rstest]
        fn test_search_respects_limit() {
            let results = search_emojis_by_shortcode("a", 5);
            assert!(results.len() <= 5);
        }

        #[rstest]
        fn test_search_returns_emoji_chars() {
            let results = search_emojis_by_shortcode("rocket", 5);
            let rocket = results.iter().find(|(sc, _)| sc == "rocket");
            assert!(rocket.is_some());
            assert_eq!(rocket.unwrap().1, "🚀");
        }
    }
}
