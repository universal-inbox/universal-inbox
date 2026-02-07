#![allow(non_snake_case)]

use dioxus::prelude::*;
use slack_blocks_render::SlackReferences;
use slack_morphism::prelude::*;

#[component]
pub fn SlackReactions(
    reactions: ReadSignal<Vec<SlackReaction>>,
    slack_references: ReadSignal<SlackReferences>,
) -> Element {
    if reactions().is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "flex flex-wrap gap-2 my-2",

            for reaction in reactions() {
                div {
                    class: "flex badge gap-1 bg-secondary text-secondary-content text-sm",

                    SlackEmojiDisplay {
                        emoji_name: reaction.name.0,
                        slack_references
                    },

                    span { "{reaction.count}" }
                }
            }
        }
    }
}

#[component]
pub fn SlackEmojiDisplay(
    emoji_name: ReadSignal<String>,
    slack_references: ReadSignal<SlackReferences>,
) -> Element {
    let emoji =
        use_memo(move || render_emoji(&SlackEmojiName(emoji_name()), &slack_references(), "h-5"));

    rsx! { { emoji } }
}

fn render_emoji(
    emoji_name: &SlackEmojiName,
    slack_references: &SlackReferences,
    class: &str,
) -> Element {
    if let Some(Some(emoji)) = slack_references.emojis.get(emoji_name) {
        match emoji {
            SlackEmojiRef::Alias(alias) => {
                return render_emoji(alias, slack_references, class);
            }
            SlackEmojiRef::Url(url) => {
                return rsx! {
                    img { class, src: "{url}" }
                };
            }
        }
    }
    let name = &emoji_name.0;

    let splitted = name.split("::skin-tone-").collect::<Vec<&str>>();
    let Some(first) = splitted.first() else {
        return rsx! { span { class, ":{name}:" } };
    };
    let Some(emoji) = emojis::get_by_shortcode(first) else {
        return rsx! { span { class, ":{name}:" } };
    };
    let Some(skin_tone) = splitted.get(1).and_then(|s| s.parse::<usize>().ok()) else {
        return rsx! { span { class, "{emoji}" } };
    };
    let Some(mut skin_tones) = emoji.skin_tones() else {
        return rsx! { span { class, "{emoji}" } };
    };
    let Some(skinned_emoji) = skin_tones.nth(skin_tone - 1) else {
        return rsx! { span { class, "{emoji}" } };
    };

    rsx! { span { class, "{skinned_emoji}" } }
}
