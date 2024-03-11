#![allow(non_snake_case)]

use comrak::{markdown_to_html as md2html, Options};
use dioxus::prelude::*;

#[component]
pub fn Markdown<'a>(cx: Scope, text: String, class: Option<&'a str>) -> Element {
    let class = class.unwrap_or("dark:prose-invert");
    render! {
        p {
            class: "w-full prose prose-sm {class}",
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
    md2html(text, &markdown_opts)
}
