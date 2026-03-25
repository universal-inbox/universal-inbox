#![allow(non_snake_case)]

use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use slack_blocks_render::SlackReferences;
use slack_morphism::prelude::*;

use universal_inbox::third_party::integrations::slack::{
    SlackMessageRender, SlackMessageSenderDetails, SlackThread,
};

use crate::components::{
    integrations::slack::{
        SlackTeamDisplay, get_sender_name_and_avatar, preview::reactions::ThreadReactionChip,
    },
    markdown::SlackHtml,
    preview_card_header::PreviewCardHeader,
    thread::ThreadDivider,
    ui::{
        participant_stack::{ParticipantDescriptor, ParticipantStack as UiParticipantStack},
        thread_message::{ThreadedMessage, ThreadedMessageFollowup},
    },
};

const GROUP_GAP_SECS: i64 = 5 * 60;
const COLLAPSE_THRESHOLD: usize = 3;

#[component]
pub fn SlackThreadPreview(
    slack_thread: ReadSignal<SlackThread>,
    title: ReadSignal<String>,
    expand_details: ReadSignal<bool>,
) -> Element {
    // `title` is computed by the parent NotificationDetails block; the thread body
    // doesn't render it (read-only triage view). Kept in props for stable contract.
    let _ = title;

    let mut show_all = use_signal(|| false);
    let _resource = use_resource(move || async move {
        *show_all.write() = expand_details();
    });

    let thread = slack_thread();
    let root = thread.messages.first().clone();
    let replies: Vec<SlackHistoryMessage> = thread.messages.iter().skip(1).cloned().collect();

    let split_index = match thread.last_read.as_ref() {
        Some(ts) => replies
            .iter()
            .position(|m| &m.origin.ts == ts)
            .map(|i| i + 1)
            .unwrap_or(0),
        None => 0,
    };
    let read_replies: Vec<SlackHistoryMessage> = replies[..split_index].to_vec();
    let unread_replies: Vec<SlackHistoryMessage> = replies[split_index..].to_vec();
    let unread_count = unread_replies.len();
    let reply_count = replies.len();

    let participants = compute_participants(&thread);

    let collapse_active = !show_all() && read_replies.len() >= COLLAPSE_THRESHOLD;
    let read_groups: Vec<Vec<SlackHistoryMessage>> = if collapse_active {
        Vec::new()
    } else {
        group_messages(&read_replies)
    };
    let unread_groups: Vec<Vec<SlackHistoryMessage>> = group_messages(&unread_replies);

    let collapsed_first = read_replies.first().cloned();
    let collapsed_last = if read_replies.len() > 1 {
        read_replies.last().cloned()
    } else {
        None
    };
    let hidden_count = read_replies.len().saturating_sub(2);

    let sender_profiles = thread.sender_profiles.clone();
    let references = thread.references.clone();
    let user_slack_id = thread.user_slack_id.clone();
    let channel = thread.channel.clone();
    let team = thread.team.clone();

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            ThreadHead {
                channel: channel.clone(),
                team,
                reply_count,
                participants: participants.clone(),
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                div {
                    class: "preview-card",

                    SlackMessageGroup {
                        messages: vec![root.clone()],
                        sender_profiles: sender_profiles.clone(),
                        references: references.clone(),
                        user_slack_id: user_slack_id.clone(),
                    }

                    if collapse_active {
                        if let Some(first) = collapsed_first.clone() {
                            SlackMessageGroup {
                                messages: vec![first],
                                sender_profiles: sender_profiles.clone(),
                                references: references.clone(),
                                user_slack_id: user_slack_id.clone(),
                            }
                        }
                        if hidden_count > 0 {
                            ThreadDivider {
                                button {
                                    r#type: "button",
                                    onclick: move |_| { *show_all.write() = true; },
                                    if hidden_count == 1 {
                                        "1 hidden reply…"
                                    } else {
                                        "{hidden_count} hidden replies…"
                                    }
                                }
                            }
                        }
                        if let Some(last) = collapsed_last.clone() {
                            SlackMessageGroup {
                                messages: vec![last],
                                sender_profiles: sender_profiles.clone(),
                                references: references.clone(),
                                user_slack_id: user_slack_id.clone(),
                            }
                        }
                    } else {
                        for group in read_groups.iter().cloned() {
                            SlackMessageGroup {
                                messages: group,
                                sender_profiles: sender_profiles.clone(),
                                references: references.clone(),
                                user_slack_id: user_slack_id.clone(),
                            }
                        }
                    }

                    if unread_count > 0 {
                        ThreadDivider {
                            unread: true,
                            if unread_count == 1 { "1 NEW REPLY" } else { "{unread_count} NEW REPLIES" }
                        }
                        for group in unread_groups.iter().cloned() {
                            SlackMessageGroup {
                                messages: group,
                                sender_profiles: sender_profiles.clone(),
                                references: references.clone(),
                                user_slack_id: user_slack_id.clone(),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ThreadHead(
    channel: ReadSignal<SlackChannelInfo>,
    team: ReadSignal<SlackTeamInfo>,
    reply_count: ReadSignal<usize>,
    participants: ReadSignal<Vec<(String, String)>>,
) -> Element {
    let ch = channel();
    let channel_name = ch.name.clone().unwrap_or_else(|| ch.id.to_string());
    let is_private = ch.flags.is_private.unwrap_or(false);
    let count = reply_count();
    let reply_word = if count == 1 { "reply" } else { "replies" };
    let parts = participants();

    rsx! {
        PreviewCardHeader {
            brand_icon: rsx! { span { class: "icon-[lucide--messages-square] size-4" } },
            title: format!("#{channel_name}"),
            subline: rsx! {
                SlackTeamDisplay { team: team() }
                span { class: "sep", "·" }
                span { "Thread · {count} {reply_word}" }
                if is_private {
                    span { class: "sep", "·" }
                    span { class: "icon-[lucide--lock] size-3" }
                    span { "Private" }
                }
                if !parts.is_empty() {
                    span { class: "sep", "·" }
                    ParticipantStack { participants: parts }
                }
            },
        }
    }
}

#[component]
fn ParticipantStack(participants: ReadSignal<Vec<(String, String)>>) -> Element {
    const MAX_DOTS: usize = 4;
    let descriptors: Vec<ParticipantDescriptor> = participants()
        .into_iter()
        .map(|(id, name)| {
            let hue = user_hue(&id);
            let color = format!("hsl({hue} 70% 62%)");
            ParticipantDescriptor { id, name, color }
        })
        .collect();

    rsx! {
        UiParticipantStack { participants: descriptors, max_visible: MAX_DOTS }
    }
}

/// Renders a same-author message group: lead message via [`ThreadedMessage`]
/// (with avatar + header) and any followup messages via
/// [`ThreadedMessageFollowup`] (compact body-only rows).
#[component]
fn SlackMessageGroup(
    messages: ReadSignal<Vec<SlackHistoryMessage>>,
    sender_profiles: ReadSignal<HashMap<String, SlackMessageSenderDetails>>,
    references: ReadSignal<Option<SlackReferences>>,
    user_slack_id: ReadSignal<Option<String>>,
) -> Element {
    let msgs = messages();
    if msgs.is_empty() {
        return rsx! {};
    }
    let head = msgs[0].clone();
    let tail: Vec<SlackHistoryMessage> = msgs.iter().skip(1).cloned().collect();
    let profiles = sender_profiles();
    let refs = references();
    let me = user_slack_id();

    let id = sender_id_string(&head.sender).unwrap_or_else(|| "?".to_string());
    let (author_name, author_avatar) = profiles
        .get(&id)
        .map(get_sender_name_and_avatar)
        .unwrap_or_else(|| (id.clone(), None));

    let head_sent_at = head.origin.ts.to_date_time_opt();

    rsx! {
        // `ui-thread-msg-grouped` is a class hook for the followup-time
        // hover cascade and the `> .ui-thread-msg-followup` 38px indent;
        // layout (flex column) lives as utilities here.
        div {
            class: "ui-thread-msg-grouped flex flex-col",

            ThreadedMessage {
                author_name,
                author_avatar_url: author_avatar,
                sent_at: head_sent_at,
                body: rsx! {
                    SlackBody {
                        message: head,
                        sender_profiles: profiles.clone(),
                        references: refs.clone(),
                        user_slack_id: me.clone(),
                    }
                },
            }

            for follow in tail.iter().cloned() {
                {
                    let follow_ts = follow.origin.ts.to_date_time_opt();
                    rsx! {
                        ThreadedMessageFollowup {
                            sent_at: follow_ts,
                            body: rsx! {
                                SlackBody {
                                    message: follow,
                                    sender_profiles: profiles.clone(),
                                    references: refs.clone(),
                                    user_slack_id: me.clone(),
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SlackBody(
    message: ReadSignal<SlackHistoryMessage>,
    sender_profiles: ReadSignal<HashMap<String, SlackMessageSenderDetails>>,
    references: ReadSignal<Option<SlackReferences>>,
    user_slack_id: ReadSignal<Option<String>>,
) -> Element {
    let msg = message();
    let html = msg.render_content_as_html(
        references(),
        "text-primary",
        "text-warning",
        user_slack_id(),
    );
    let reactions = msg.content.reactions.clone();
    let refs = references().unwrap_or_default();
    let profiles = sender_profiles();
    let me = user_slack_id();

    rsx! {
        SlackHtml { class: "prose prose-sm max-w-none", html }
        if let Some(rxns) = reactions {
            if !rxns.is_empty() {
                // `.th-rxns` shell migrated to utilities: flex wrap + gap + top margin.
                div {
                    class: "flex flex-wrap gap-1 mt-1.5",
                    for rxn in rxns {
                        ThreadReactionChip {
                            reaction: rxn,
                            slack_references: refs.clone(),
                            current_user_id: me.clone(),
                            sender_profiles: profiles.clone(),
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers

fn compute_participants(thread: &SlackThread) -> Vec<(String, String)> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<(String, String)> = Vec::new();
    for msg in thread.messages.iter() {
        if let Some(id) = sender_id_string(&msg.sender)
            && seen.insert(id.clone())
        {
            let name = thread
                .sender_profiles
                .get(&id)
                .map(|p| get_sender_name_and_avatar(p).0)
                .unwrap_or_else(|| id.clone());
            out.push((id, name));
        }
    }
    out
}

fn group_messages(messages: &[SlackHistoryMessage]) -> Vec<Vec<SlackHistoryMessage>> {
    let mut groups: Vec<Vec<SlackHistoryMessage>> = Vec::new();
    for msg in messages {
        let start_new = match groups.last() {
            None => true,
            Some(g) => {
                let prev = g.last().expect("group is non-empty");
                let prev_id = sender_id_string(&prev.sender);
                let cur_id = sender_id_string(&msg.sender);
                let same_author = prev_id.is_some() && prev_id == cur_id;
                let close = match (
                    prev.origin.ts.to_date_time_opt(),
                    msg.origin.ts.to_date_time_opt(),
                ) {
                    (Some(p), Some(c)) => (c - p).num_seconds() <= GROUP_GAP_SECS,
                    _ => false,
                };
                !(same_author && close)
            }
        };
        if start_new {
            groups.push(vec![msg.clone()]);
        } else {
            groups.last_mut().unwrap().push(msg.clone());
        }
    }
    groups
}

fn sender_id_string(sender: &SlackMessageSender) -> Option<String> {
    sender
        .user
        .as_ref()
        .map(|u| u.to_string())
        .or_else(|| sender.bot_id.as_ref().map(|b| b.to_string()))
}

fn user_hue(user_id: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for b in user_id.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h % 360
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn user_hue_is_deterministic() {
        let a = user_hue("U123");
        let b = user_hue("U123");
        assert_eq!(a, b);
        assert!(a < 360);
    }

    #[wasm_bindgen_test]
    fn user_hue_differs_per_id() {
        assert_ne!(user_hue("U001"), user_hue("U002"));
    }
}
