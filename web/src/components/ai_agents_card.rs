#![allow(non_snake_case)]

//! "AI Agents (MCP)" settings card.
//!
//! Rendered in the **Utility services** section of the integrations settings
//! page. Unlike the other integration cards there is no *Connect* button: an
//! AI agent connection is initiated *by the agent* (it registers as an OAuth2
//! client and runs the authorization-code flow against our MCP server). So the
//! card shows a **Documentation** button instead, marks itself **Connected**
//! once at least one MCP client has authorized, and lists each connected agent
//! with a per-agent icon and a Revoke action.
//!
//! Membership is decided server-side by [`AuthorizedOAuth2Client::is_mcp`]
//! (true when the client holds a grant whose effective resource targets the
//! MCP server). The per-agent icon is chosen from the client's metadata
//! (`client_name` / `software_id`) — the agent's **official** brand logo from
//! the Iconify `logos` set for well-known agents, and the Model Context
//! Protocol mark as the generic fallback.

use dioxus::prelude::*;

use universal_inbox::auth::oauth2::AuthorizedOAuth2Client;

use crate::{
    components::ui::{
        Button, ButtonVariant, Card, CardBody, CardHeader, CardMeta, CardRight, CardVariant,
        StatusLeaf, StatusLeafVariant,
    },
    services::oauth2_client_service::{OAUTH2_AUTHORIZED_CLIENTS, OAuth2ClientCommand},
};

/// External documentation page describing how to connect an AI agent to the
/// Universal Inbox MCP server.
const MCP_DOCUMENTATION_URL: &str = "https://doc.universal-inbox.com/misc/ai_agents";

/// Generic AI-agent icon used when the client doesn't match a known agent.
/// The official Model Context Protocol mark — every client in this card is an
/// MCP client, so it's the natural fallback. Multi-color `logos:*` glyphs keep
/// their native palette (the text color is ignored), matching `BrandTile`.
const GENERIC_AGENT_ICON: &str = "icon-[logos--model-context-protocol-icon]";

/// Curated `(keyword, icon-class)` table mapping a well-known agent — matched
/// case-insensitively as a substring of `client_name` or `software_id` — to
/// its **official** brand logo from the Iconify `logos` set. Extend this list
/// as new agents appear. The first match wins; unknown agents fall back to the
/// generic MCP mark.
const KNOWN_AGENT_ICONS: &[(&str, &str)] = &[
    ("claude", "icon-[logos--claude-icon]"),
    ("anthropic", "icon-[logos--claude-icon]"),
    ("chatgpt", "icon-[logos--openai-icon]"),
    ("openai", "icon-[logos--openai-icon]"),
    ("copilot", "icon-[logos--github-copilot]"),
    ("perplexity", "icon-[logos--perplexity-icon]"),
    ("mistral", "icon-[logos--mistral-ai-icon]"),
    ("vscode", "icon-[logos--visual-studio-code]"),
    ("vs code", "icon-[logos--visual-studio-code]"),
    ("visual studio code", "icon-[logos--visual-studio-code]"),
];

/// Pick an icon class for an agent from its metadata. Dedicated icon for a
/// known agent (matched against `client_name` then `software_id`), otherwise
/// [`GENERIC_AGENT_ICON`].
fn agent_icon(client_name: Option<&str>, software_id: Option<&str>) -> &'static str {
    for hay in [client_name, software_id].into_iter().flatten() {
        let hay = hay.to_ascii_lowercase();
        for (keyword, icon) in KNOWN_AGENT_ICONS {
            if hay.contains(keyword) {
                return icon;
            }
        }
    }
    GENERIC_AGENT_ICON
}

#[component]
pub fn AiAgentsCard() -> Element {
    let oauth2_client_service = use_coroutine_handle::<OAuth2ClientCommand>();

    // Ensure the authorized-clients signal is populated even if the user
    // landed directly on the settings page (the Security page also refreshes
    // it). Both pages share the same global signal + coroutine.
    let _resource = use_resource(move || {
        to_owned![oauth2_client_service];
        async move {
            oauth2_client_service.send(OAuth2ClientCommand::Refresh);
        }
    });

    // Only MCP clients belong in this card; other OAuth2 clients (e.g. a
    // future Raycast REST client) are surfaced elsewhere.
    let agents: Vec<AuthorizedOAuth2Client> = OAUTH2_AUTHORIZED_CLIENTS
        .read()
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|client| client.is_mcp)
        .collect();

    let connected = !agents.is_empty();
    let (status_variant, status_label) = if connected {
        (StatusLeafVariant::Connected, "Connected")
    } else {
        (StatusLeafVariant::Disconnected, "Not connected")
    };

    let header_description = if !connected {
        "Connect an AI agent to the MCP tools".to_string()
    } else if agents.len() == 1 {
        "1 agent connected".to_string()
    } else {
        format!("{} agents connected", agents.len())
    };

    let mut is_expanded = use_signal(|| false);
    // Only the connected card expands (it has agents to reveal); the
    // disconnected card is a static header.
    let card_expanded = connected && is_expanded();

    // Documentation link — placed left of the status badge in both states. In
    // the connected (clickable) header it sits inside a wrapper that stops the
    // click from bubbling to the row's expand toggle (and `target=_blank`
    // navigation still fires as the default action).
    let documentation_button = rsx! {
        Button {
            variant: ButtonVariant::Ghost,
            icon_class: "icon-[lucide--book-open]".to_string(),
            href: MCP_DOCUMENTATION_URL.to_string(),
            aria_label: "AI agents documentation".to_string(),
            "Documentation"
        }
    };

    rsx! {
        Card {
            variant: CardVariant::Integration,
            expanded: card_expanded,
            class: if connected { String::new() } else { "disconnected-card".to_string() },

            if connected {
                // Click-target header — mirrors `IntegrationSettings`' connected
                // header so the whole row toggles the expandable body.
                div {
                    class: "group flex items-center gap-2.5 px-3.5 py-3 cursor-pointer \
                            select-none transition-colors duration-[var(--ui-dur-fast)] \
                            hover:bg-ui-surface-hover focus-visible:outline-2 \
                            focus-visible:outline-ui-primary focus-visible:-outline-offset-2 \
                            focus-visible:rounded-ui-lg max-md:flex-wrap",
                    role: "button",
                    tabindex: 0,
                    aria_expanded: "{is_expanded}",
                    aria_label: "Toggle AI Agents settings",
                    onclick: move |_| is_expanded.toggle(),
                    onkeydown: move |event: KeyboardEvent| {
                        if event.key() == Key::Enter || event.key() == Key::Character(" ".to_string()) {
                            event.prevent_default();
                            is_expanded.toggle();
                        }
                    },

                    div {
                        class: "flex items-center justify-center shrink-0 size-[26px] bg-transparent border border-ui-border rounded-ui-sm",
                        span { class: "icon-[logos--model-context-protocol-icon] size-4" }
                    }

                    CardMeta {
                        name: "AI Agents (MCP)".to_string(),
                        description: rsx! { "{header_description}" },
                        hide_description: is_expanded(),
                    }

                    CardRight {
                        class: "max-md:basis-full max-md:mt-1".to_string(),
                        // Wrapper stops the toggle from firing when the link is
                        // clicked; the link still opens (default action).
                        div {
                            onclick: move |event| event.stop_propagation(),
                            {documentation_button.clone()}
                        }
                        StatusLeaf { variant: status_variant, label: status_label.to_string() }
                        span {
                            class: if is_expanded() {
                                "size-6 inline-flex items-center justify-center rounded-full \
                                 text-ui-base-muted shrink-0 transition-transform duration-150 \
                                 rotate-180 group-hover:bg-ui-base-200"
                            } else {
                                "size-6 inline-flex items-center justify-center rounded-full \
                                 text-ui-base-muted shrink-0 transition-transform duration-150 \
                                 group-hover:bg-ui-base-200"
                            },
                            span { class: "icon-[lucide--chevron-down] size-4" }
                        }
                    }
                }

                CardBody {
                    expandable: true,
                    div {
                        class: "connections-list px-3.5 pb-3 flex flex-col gap-1",
                        for agent in agents.into_iter() {
                            AiAgentRow {
                                key: "{agent.client_id}",
                                client_id: agent.client_id.clone(),
                                client_name: agent.client_name.clone(),
                                software_id: agent.software_id.clone(),
                                last_used_at: agent.last_used_at,
                            }
                        }
                    }
                }
            } else {
                CardHeader {
                    interactive: false,
                    class: "max-md:flex-wrap".to_string(),

                    div {
                        class: "flex items-center justify-center shrink-0 size-[26px] bg-transparent border border-ui-border rounded-ui-sm",
                        span { class: "icon-[logos--model-context-protocol-icon] size-4" }
                    }

                    CardMeta {
                        name: "AI Agents (MCP)".to_string(),
                        description: rsx! { "{header_description}" },
                        muted_name: true,
                    }

                    CardRight {
                        class: "max-md:basis-full max-md:mt-1".to_string(),
                        {documentation_button}
                        StatusLeaf { variant: status_variant, label: status_label.to_string() }
                    }
                }
            }
        }
    }
}

#[component]
fn AiAgentRow(
    client_id: String,
    client_name: Option<String>,
    software_id: Option<String>,
    last_used_at: chrono::DateTime<chrono::Utc>,
) -> Element {
    let oauth2_client_service = use_coroutine_handle::<OAuth2ClientCommand>();
    let mut confirming = use_signal(|| false);

    let icon = agent_icon(client_name.as_deref(), software_id.as_deref());
    let display_name = client_name
        .clone()
        .unwrap_or_else(|| format!("{}…", &client_id[..client_id.len().min(8)]));

    rsx! {
        div {
            class: "flex items-center gap-2 px-3 py-2 bg-ui-base-200 border border-ui-border-light \
                    rounded-ui-sm text-[length:var(--ui-text-sm)] text-ui-base-content",

            span { class: "{icon} size-4 shrink-0 text-ui-base-muted" }
            span { class: "flex-1 min-w-0 truncate font-medium", "{display_name}" }
            span {
                class: "shrink-0 text-[11px] text-ui-base-muted max-md:hidden",
                r#"used {last_used_at.date_naive().format("%Y-%m-%d")}"#
            }
            div {
                class: "ml-auto inline-flex gap-1.5 shrink-0",
                if confirming() {
                    Button {
                        variant: ButtonVariant::Danger,
                        onclick: {
                            let client_id = client_id.clone();
                            move |_| {
                                oauth2_client_service.send(OAuth2ClientCommand::RevokeClient(client_id.clone()));
                                confirming.set(false);
                            }
                        },
                        "Confirm"
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| confirming.set(false),
                        "Cancel"
                    }
                } else {
                    Button {
                        variant: ButtonVariant::Danger,
                        icon_class: "icon-[lucide--trash-2]".to_string(),
                        onclick: move |_| confirming.set(true),
                        span { class: "hidden sm:inline", "Revoke" }
                    }
                }
            }
        }
    }
}
