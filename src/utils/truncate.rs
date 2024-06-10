use unicode_segmentation::UnicodeSegmentation;

pub fn truncate_with_ellipse(
    s: &str,
    len: usize,
    ellipse: &str,
    keep_only_first_line: bool,
) -> String {
    let mut lines = s.lines();
    if keep_only_first_line {
        let first_line = lines.next().unwrap_or(s);

        if first_line.graphemes(true).count() <= len {
            if lines.next().is_some() {
                return format!("{first_line}{ellipse}");
            }
            return first_line.to_string();
        }
    }

    let graphemes = s.graphemes(true);
    if graphemes.clone().count() <= len {
        return s.to_string();
    }

    graphemes.take(len).chain(ellipse.graphemes(true)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[rstest]
    fn test_full_string() {
        let s = "Hello, world!";
        assert_eq!(truncate_with_ellipse(s, 20, "...", false), "Hello, world!");
    }

    #[rstest]
    fn test_truncated_string() {
        let s = "Hello, world!";
        assert_eq!(truncate_with_ellipse(s, 5, "...", false), "Hello...");
    }

    #[rstest]
    fn test_multiline_full_string() {
        let s = "Hello, world!\nHello, world!";
        assert_eq!(
            truncate_with_ellipse(s, 42, "...", false),
            "Hello, world!\nHello, world!"
        );
    }

    #[rstest]
    fn test_multiline_truncated_string() {
        let s = "Hello, world!\nHello, world!";
        assert_eq!(truncate_with_ellipse(s, 5, "...", false), "Hello...");
    }

    #[rstest]
    fn test_multiline_string_keep_only_first_line() {
        let s = "Hello, world!\nHello, world!";
        assert_eq!(
            truncate_with_ellipse(s, 42, "...", true),
            "Hello, world!..."
        );
    }
}
