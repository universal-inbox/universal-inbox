//! Async Slack emoji picker, used in the Slack reaction config to pin the
//! "sync" and "completion" emojis. Looks up the emoji glyph from the
//! `emojis` crate's shortcode registry so each row shows the unicode tile
//! next to the `:shortcode:` label.

#![allow(non_snake_case)]

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use log::error;
use url::Url;

use universal_inbox::integration_connection::{
    IntegrationConnectionId, integrations::slack::SlackEmojiSuggestion,
};

use crate::components::ui::{
    EmojiOption, EmojiValue, SEARCH_DEBOUNCE_MS, UISearchSelect, UISelectOption,
    search_slack_emojis,
};

fn emoji_glyph(name: &str) -> String {
    emojis::get_by_shortcode(name)
        .map(|e| e.as_str().to_string())
        .unwrap_or_default()
}

#[component]
pub fn EmojiSearchField(
    api_base_url: ReadSignal<Url>,
    connection_id: ReadSignal<IntegrationConnectionId>,
    selected: Signal<Option<SlackEmojiSuggestion>>,
    on_change: EventHandler<Option<SlackEmojiSuggestion>>,
    name: String,
    #[props(default = "Pick an emoji…".to_string())] placeholder: String,
    #[props(default = false)] disabled: bool,
    #[props(default)] width: Option<String>,
    #[props(default = true)] allow_clear: bool,
) -> Element {
    let mut query = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut options = use_signal(Vec::<UISelectOption<SlackEmojiSuggestion>>::new);

    let _resource = use_resource(move || async move {
        let q = query();
        if q.trim().is_empty() {
            options.set(Vec::new());
            loading.set(false);
            return;
        }
        loading.set(true);
        TimeoutFuture::new(SEARCH_DEBOUNCE_MS).await;
        if query() != q {
            return;
        }
        match search_slack_emojis(&api_base_url(), connection_id(), &q).await {
            Ok(results) => options.set(results),
            Err(err) => {
                error!("Failed to search Slack emojis: {err:?}");
                options.set(Vec::new());
            }
        }
        loading.set(false);
    });

    let render_value = use_callback(move |opt: UISelectOption<SlackEmojiSuggestion>| {
        let glyph = emoji_glyph(&opt.value.name);
        rsx! { EmojiValue { emoji: glyph, label: opt.label } }
    });
    let render_option = use_callback(
        move |(opt, q): (UISelectOption<SlackEmojiSuggestion>, String)| {
            let glyph = emoji_glyph(&opt.value.name);
            rsx! { EmojiOption { emoji: glyph, label: opt.label, query: q } }
        },
    );

    // `UISearchSelect` resolves the trigger label by looking up `value` inside
    // `options`. Server-filtered options start empty, so we pin the current
    // selection as a synthetic option to make the saved value render on load.
    let display_options = use_memo(move || {
        let mut opts = options();
        if let Some(sel) = selected().as_ref()
            && !opts.iter().any(|o| &o.value == sel)
        {
            let label = format!(":{}:", sel.name);
            opts.insert(0, UISelectOption::new(sel.clone(), label));
        }
        opts
    });

    rsx! {
        UISearchSelect::<SlackEmojiSuggestion> {
            value: selected,
            options: display_options(),
            on_change: move |emoji: Option<SlackEmojiSuggestion>| { on_change.call(emoji); },
            on_query: move |q: String| { query.set(q); },
            loading: loading(),
            placeholder,
            search_placeholder: "Type to search emoji…".to_string(),
            empty_hint: "Type to search Slack emojis.".to_string(),
            allow_clear,
            disabled,
            width,
            name,
            render_value,
            render_option,
        }
    }
}
