#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use slack_blocks_render::SlackReferences;
use slack_morphism::prelude::*;

use universal_inbox::third_party::integrations::slack::SlackMessageSenderDetails;

use crate::components::{
    flyonui::tooltip::{Tooltip, TooltipPlacement},
    integrations::slack::get_sender_name_and_avatar,
    ui::reaction_chip::{ReactionChip, ReactionVariant},
};

#[component]
pub fn SlackReactions(
    reactions: ReadSignal<Vec<SlackReaction>>,
    slack_references: ReadSignal<SlackReferences>,
) -> Element {
    if reactions().is_empty() {
        return rsx! {};
    }

    rsx! {
        // `.th-rxns` shell migrated to utilities: flex wrap + gap + top margin.
        div {
            class: "flex flex-wrap gap-1 mt-1.5",

            for reaction in reactions() {
                ReactionChip {
                    emoji: rsx! {
                        SlackEmojiDisplay {
                            emoji_name: reaction.name.0,
                            slack_references,
                        }
                    },
                    count: reaction.count as u32,
                }
            }
        }
    }
}

/// Read-only reaction chip used by the Slack thread renderer. No add-reaction
/// affordance, no click handler — just a tooltip listing reactors.
#[component]
pub fn ThreadReactionChip(
    reaction: ReadSignal<SlackReaction>,
    slack_references: ReadSignal<SlackReferences>,
    current_user_id: ReadSignal<Option<String>>,
    sender_profiles: ReadSignal<HashMap<String, SlackMessageSenderDetails>>,
) -> Element {
    let r = reaction();
    let is_mine = current_user_id()
        .as_ref()
        .map(|me| r.users.iter().any(|u| u.to_string() == *me))
        .unwrap_or(false);

    let profiles = sender_profiles();
    let names: Vec<String> = r
        .users
        .iter()
        .map(|u| {
            let id = u.to_string();
            profiles
                .get(&id)
                .map(|p| get_sender_name_and_avatar(p).0)
                .unwrap_or(id)
        })
        .collect();
    let tooltip = format!("{} reacted with :{}:", names.join(", "), r.name.0);
    let variant = if is_mine {
        ReactionVariant::Mine
    } else {
        ReactionVariant::Default
    };

    rsx! {
        Tooltip {
            text: tooltip,
            placement: TooltipPlacement::Top,
            ReactionChip {
                emoji: rsx! {
                    SlackEmojiDisplay {
                        emoji_name: r.name.0.clone(),
                        slack_references,
                    }
                },
                count: r.count as u32,
                variant,
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
