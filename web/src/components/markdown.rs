#![allow(non_snake_case)]

use comrak::{markdown_to_html as md2html, Options};
use dioxus::prelude::*;
use regex::Regex;

#[component]
pub fn Markdown(text: String, class: Option<String>) -> Element {
    let class = class.unwrap_or("dark:prose-invert".to_string());
    rsx! {
        p {
            class: "w-full max-w-full prose prose-sm {class}",
            dangerous_inner_html: "{markdown_to_html(&text)}"
        }
    }
}

pub fn markdown_to_html(text: &str) -> String {
    let mut markdown_opts = Options::default();
    markdown_opts.extension.strikethrough = true;
    markdown_opts.extension.table = true;
    markdown_opts.extension.tasklist = true;
    markdown_opts.extension.shortcodes = true;
    markdown_opts.render.escape = true;

    let html = md2html(text, &markdown_opts);
    let re = Regex::new(r"@(@[^@]+)@").unwrap();
    re.replace_all(&html, "<span class=\"text-primary\">$1</span>")
        .to_string()
}

#[cfg(test)]
mod markdown_to_html_tests {
    use super::*;
    use wasm_bindgen_test::*;

    mod quoted_text {
        use super::*;
        use pretty_assertions::assert_eq;

        #[wasm_bindgen_test]
        fn test_markdown_to_html_quoted_text_followed_by_text() {
            assert_eq!(
                markdown_to_html("> Test1\n\nTest2"),
                "<blockquote>\n<p>Test1</p>\n</blockquote>\n<p>Test2</p>\n".to_string()
            );
        }

        #[wasm_bindgen_test]
        fn test_markdown_to_html_quoted_text_followed_by_quoted_text() {
            assert_eq!(
                markdown_to_html("> Test1\n> Test2"),
                "<blockquote>\n<p>Test1\nTest2</p>\n</blockquote>\n".to_string()
            );
        }

        #[wasm_bindgen_test]
        fn test_markdown_to_html_quoted_text_with_newline() {
            assert_eq!(
                markdown_to_html("> Test1\nTest2"),
                "<blockquote>\n<p>Test1\nTest2</p>\n</blockquote>\n".to_string()
            );
        }
    }

    mod preformatted_text {
        use super::*;
        use pretty_assertions::assert_eq;

        #[wasm_bindgen_test]
        fn test_markdown_to_html_preformatted_text_followed_by_text() {
            assert_eq!(
                markdown_to_html("```\nTest1\n```\nTest2"),
                "<pre><code>Test1\n</code></pre>\n<p>Test2</p>\n".to_string()
            );
        }

        #[wasm_bindgen_test]
        fn test_markdown_to_html_preformatted_text_followed_by_preformatted_text() {
            assert_eq!(
                markdown_to_html("```\nTest1\n```\n```\nTest2\n```"),
                "<pre><code>Test1\n</code></pre>\n<pre><code>Test2\n</code></pre>\n".to_string()
            );
        }

        #[wasm_bindgen_test]
        fn test_markdown_to_html_preformatted_text_with_newline() {
            assert_eq!(
                markdown_to_html("```\nTest1\nTest2\n```"),
                "<pre><code>Test1\nTest2\n</code></pre>\n".to_string()
            );
        }
    }
}
