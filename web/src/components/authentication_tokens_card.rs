#![allow(non_snake_case)]

use std::ops::Deref;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::{
        bs_icons::{BsKey, BsShieldLock},
        go_icons::GoCopy,
    },
    Icon,
};
use fermi::use_atom_ref;

use secrecy::ExposeSecret;
use universal_inbox::auth::auth_token::AuthenticationTokenId;

use crate::{
    components::spinner::Spinner,
    model::LoadState,
    services::authentication_token_service::{
        AuthenticationTokenCommand, AUTHENTICATION_TOKENS, CREATED_AUTHENTICATION_TOKEN,
    },
    utils::copy_to_clipboard,
};

#[component]
pub fn AuthenticationTokensCard(cx: Scope) -> Element {
    let authentication_tokens_ref = use_atom_ref(cx, &AUTHENTICATION_TOKENS);
    let created_authentication_token_ref = use_atom_ref(cx, &CREATED_AUTHENTICATION_TOKEN);
    let authentication_token_service =
        use_coroutine_handle::<AuthenticationTokenCommand>(cx).unwrap();

    use_future(cx, (), |()| {
        to_owned![authentication_token_service];

        async move {
            authentication_token_service.send(AuthenticationTokenCommand::Refresh);
        }
    });

    let Some(authentication_tokens) = authentication_tokens_ref.read().clone() else {
        return render! {
            div {
                class: "card w-full bg-base-200 text-base-content",
                div {
                    class: "h-full flex justify-center items-center",
                    Spinner {}
                    "Loading API keys..."
                }
            }
        };
    };

    render! {
        div {
            class: "card w-full bg-base-200 text-base-content",

            div {
                class: "card-body",
                div {
                    class: "flex flex-col gap-2",

                    div {
                        class: "flex flex-row justify-between items-center",
                        div {
                            class: "card-title",
                            figure { class: "p-2", Icon { class: "w-8 h-8", icon: BsShieldLock } }
                            "API keys"
                        }

                        match created_authentication_token_ref.read().deref() {
                            LoadState::Loading => render! {
                                div {
                                    class: "btn btn-primary btn-sm btn-disabled",
                                    Spinner { class: "w-4 h-4" }
                                    "Creating new API key..."
                                }
                            },
                            _  => render! {
                                button {
                                    class: "btn btn-primary btn-sm",
                                    onclick: move |_| {
                                        authentication_token_service.send(AuthenticationTokenCommand::CreateAuthenticationToken);
                                    },
                                    Icon { class: "w-4 h-4", icon: BsKey }
                                    "Create new API key"
                                }
                            }
                        }
                    }

                    table {
                        class: "table table-xs table-fixed",
                        thead {
                            tr {
                                th { class: "w-72", "ID" }
                                th { class: "w-32", "Expiration date" }
                                th { "Key" }
                                th { class: "w-32", "" }
                            }
                        }
                        tbody {
                            if let LoadState::Loaded(created_authentication_token) = created_authentication_token_ref.read().deref() {
                                render! {
                                    AuthenticationToken {
                                        id: created_authentication_token.id.clone(),
                                        expire_at: created_authentication_token.expire_at,
                                        jwt_token: created_authentication_token.jwt_token.expose_secret().to_string(),
                                        is_copiable: true
                                    }
                                }
                            }
                            for auth_token in authentication_tokens.into_iter() {
                                render! {
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
}

#[component]
pub fn AuthenticationToken(
    cx: Scope,
    id: AuthenticationTokenId,
    #[props(!optional)] expire_at: Option<DateTime<Utc>>,
    jwt_token: String,
    is_copiable: bool,
) -> Element {
    let is_copied = use_state(cx, || false);
    let (line_class, td_class) = if *is_copiable {
        (
            "bg-green-50 ring-2 ring-success/50 ring-offset-2 ring-offset-base-200 rounded-md",
            "my-0",
        )
    } else {
        ("", "my-2")
    };

    render! {
        tr {
            class: "{line_class}",
            td { "{id}" }

            if let Some(expire_at) = expire_at {
                render! {
                    td { r#"{expire_at.date_naive().format("%Y-%m-%d")}"# }
                }
            } else {
                render! {
                    td { "Never expire" }
                }
            }

            td {
                p { class: "truncate", "{jwt_token}" }
            }

            td {
                class: "flex gap-2 justify-center items-center h-8 {td_class}",

                if !is_copiable {
                    render! {
                        button {
                            class: "btn btn-sm btn-error btn-disabled",
                            onclick: move |_| {}, // TODO
                            "Revoke"
                        }
                    }
                } else if **is_copied {
                    render! {
                        div {
                            class: "badge badge-outline badge-ghost badge-sm",
                            "Copied!"
                        }
                    }
                } else {
                    render! {
                        button {
                            class: "btn btn-ghost btn-sm",
                            onclick: move |_| {
                                let jwt_token = jwt_token.clone();
                                let is_copied = is_copied.clone();
                                async move {
                                    copy_to_clipboard(&jwt_token).await.unwrap();
                                    is_copied.set(true);
                                }
                            },
                            Icon { class: "w-4 h-4", icon: GoCopy }
                        }
                    }
                }
            }
        }
    }
}
