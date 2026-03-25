#![allow(non_snake_case)]

use std::ops::Deref;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

use secrecy::ExposeSecret;
use universal_inbox::auth::auth_token::AuthenticationTokenId;

use crate::{
    components::{
        loading::Loading,
        spinner::Spinner,
        ui::{
            Button, ButtonVariant, Card, CardEmptyState, CardHeader, CardMeta, CardRight,
            CardVariant,
        },
    },
    model::LoadState,
    services::authentication_token_service::{
        AUTHENTICATION_TOKENS, AuthenticationTokenCommand, CREATED_AUTHENTICATION_TOKEN,
    },
    utils::copy_to_clipboard,
};

#[component]
pub fn AuthenticationTokensCard() -> Element {
    let authentication_token_service = use_coroutine_handle::<AuthenticationTokenCommand>();

    let _resource = use_resource(move || {
        to_owned![authentication_token_service];

        async move {
            authentication_token_service.send(AuthenticationTokenCommand::Refresh);
        }
    });

    let Some(authentication_tokens) = AUTHENTICATION_TOKENS.read().clone() else {
        return rsx! {
            Card { variant: CardVariant::ApiKeys,
                Loading { label: "Loading API keys..." }
            }
        };
    };

    rsx! {
        section {
            role: "region",
            aria_label: "API keys",

            Card { variant: CardVariant::ApiKeys,
                CardHeader {
                    span { class: "icon-[lucide--shield-check] size-5" }
                    CardMeta { name: "Authentication tokens" }

                    CardRight {
                        match CREATED_AUTHENTICATION_TOKEN.read().deref() {
                            LoadState::Loading => rsx! {
                                Button {
                                    variant: ButtonVariant::Primary,
                                    disabled: true,
                                    Spinner { class: "w-4 h-4" }
                                    "Creating new API key..."
                                }
                            },
                            _ => rsx! {
                                Button {
                                    variant: ButtonVariant::Primary,
                                    icon_class: "icon-[lucide--key]".to_string(),
                                    onclick: move |_| {
                                        authentication_token_service.send(AuthenticationTokenCommand::CreateAuthenticationToken);
                                    },
                                    "Create new API key"
                                }
                            }
                        }
                    }
                }

                if authentication_tokens.is_empty() && !matches!(CREATED_AUTHENTICATION_TOKEN.read().deref(), LoadState::Loaded(_) | LoadState::Error(_)) {
                    CardEmptyState {
                        icon_class: "icon-[lucide--key-round]".to_string(),
                        title: "No API keys yet".to_string(),
                        description: "Create one to authenticate with the Universal Inbox API.".to_string(),
                    }
                } else {
                    table {
                        class: "api-keys-table max-md:block max-md:overflow-x-auto",
                        thead {
                            tr {
                                th { style: "width: 25%;", "Expiration date" }
                                th { "Key" }
                                th { style: "width: 20%;", aria_label: "Actions", "" }
                            }
                        }
                        tbody {
                            match CREATED_AUTHENTICATION_TOKEN.read().deref() {
                                LoadState::Loaded(created_authentication_token) => rsx! {
                                    AuthenticationToken {
                                        id: created_authentication_token.id.clone(),
                                        expire_at: created_authentication_token.expire_at,
                                        jwt_token: created_authentication_token.jwt_token.expose_secret().to_string(),
                                        is_copiable: true
                                    }
                                },
                                LoadState::Error(error) => rsx! {
                                    tr {
                                        td { colspan: "3", "Failed to create a new API key: {error}" }
                                    }
                                },
                                _ => rsx! {}
                            }
                            for auth_token in authentication_tokens.into_iter() {
                                AuthenticationToken {
                                    id: auth_token.id,
                                    expire_at: auth_token.expire_at,
                                    jwt_token: format!("**********{}", auth_token.truncated_jwt_token.clone()),
                                    is_copiable: false
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn AuthenticationToken(
    id: AuthenticationTokenId,
    #[props(!optional)] expire_at: Option<DateTime<Utc>>,
    jwt_token: String,
    is_copiable: bool,
) -> Element {
    let mut is_copied = use_signal(|| false);
    let row_class = if is_copiable { "token-new" } else { "" };

    rsx! {
        tr {
            class: "{row_class}",

            if let Some(expire_at) = expire_at {
                td { r#"{expire_at.date_naive().format("%Y-%m-%d")}"# }
            } else {
                td { "Never expire" }
            }

            td {
                span { class: "block font-mono text-[11px] text-ui-base-muted truncate", "{jwt_token}" }
            }

            td {
                div {
                    class: "flex items-center justify-end gap-1",
                    if !is_copiable {
                        Button {
                            variant: ButtonVariant::Danger,
                            disabled: true,
                            icon_class: "icon-[lucide--trash-2]".to_string(),
                            "Revoke"
                        }
                    } else if is_copied() {
                        span {
                            class: "inline-flex items-center gap-1 px-2 py-0.5 rounded-ui-pill text-xs font-medium border border-ui-border text-ui-base-muted",
                            "Copied!"
                        }
                    } else {
                        Button {
                            variant: ButtonVariant::Primary,
                            icon_class: "icon-[lucide--copy]".to_string(),
                            onclick: move |_| {
                                let jwt_token = jwt_token.clone();
                                async move {
                                    copy_to_clipboard(&jwt_token).await.unwrap();
                                    *is_copied.write() = true;
                                }
                            },
                            "Copy"
                        }
                    }
                }
            }
        }
    }
}
