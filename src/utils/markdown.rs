use url::Url;

/// Escape the characters that can end or restructure a Markdown link label
/// (`\`, `[`, `]`), so text written by a third party cannot close the label
/// early and splice in a link of its own.
pub fn escape_markdown_link_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Build `[label](url)` with an escaped label and a destination that cannot
/// close the link early (parentheses are percent-encoded).
pub fn markdown_link(label: &str, url: &Url) -> String {
    let destination = url.as_str().replace('(', "%28").replace(')', "%29");
    format!("[{}]({destination})", escape_markdown_link_text(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_markdown_link_keeps_plain_titles() {
        let url: Url = "https://linear.app/team/issue/ABC-1".parse().unwrap();
        assert_eq!(
            markdown_link("Fix the build", &url),
            "[Fix the build](https://linear.app/team/issue/ABC-1)"
        );
    }

    #[test]
    fn test_markdown_link_cannot_be_hijacked_by_the_label() {
        let url: Url = "https://linear.app/team/issue/ABC-1".parse().unwrap();
        assert_eq!(
            markdown_link("Deploy notes](https://evil.tld/login) [", &url),
            "[Deploy notes\\](https://evil.tld/login) \\[](https://linear.app/team/issue/ABC-1)"
        );
    }

    #[test]
    fn test_markdown_link_encodes_parentheses_in_the_destination() {
        let url: Url = "https://example.com/a_(b)".parse().unwrap();
        assert_eq!(
            markdown_link("x", &url),
            "[x](https://example.com/a_%28b%29)"
        );
    }
}
