#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsPeople, BsTrash},
};

use crate::{
    components::loading::Loading,
    services::oauth2_client_service::{OAUTH2_AUTHORIZED_CLIENTS, OAuth2ClientCommand},
};

#[component]
pub fn OAuthClientsCard() -> Element {
    let oauth2_client_service = use_coroutine_handle::<OAuth2ClientCommand>();

    let _resource = use_resource(move || {
        to_owned![oauth2_client_service];

        async move {
            oauth2_client_service.send(OAuth2ClientCommand::Refresh);
        }
    });

    let Some(authorized_clients) = OAUTH2_AUTHORIZED_CLIENTS.read().clone() else {
        return rsx! {
            div {
                class: "card w-full bg-base-200",
                Loading { label: "Loading authorized OAuth2 clients..." }
            }
        };
    };

    rsx! {
        div {
            class: "card w-full bg-base-200",

            div {
                class: "card-body",
                div {
                    class: "flex flex-col gap-2",

                    div {
                        class: "flex flex-col sm:flex-row justify-between items-center",
                        div {
                            class: "card-title flex flex-row items-center",
                            figure { class: "p-2", Icon { class: "w-8 h-8", icon: BsPeople } }
                            "Authorized OAuth2 clients"
                        }
                    }

                    p {
                        class: "text-sm text-base-content/70",
                        "OAuth2 clients authorized to access your Universal Inbox on your behalf."
                    }

                    if authorized_clients.is_empty() {
                        p {
                            class: "text-sm text-base-content/50",
                            "No authorized clients"
                        }
                    } else {
                        table {
                            class: "table table-xs sm:table-sm table-fixed",
                            thead {
                                tr {
                                    th { "Client name" }
                                    th { "Scope" }
                                    th { class: "w-32", "First authorized" }
                                    th { class: "w-32", "Last used" }
                                    th { class: "sm:w-32 w-8", "" }
                                }
                            }
                            tbody {
                                for client in authorized_clients.into_iter() {
                                    OAuthClientRow {
                                        client_id: client.client_id.clone(),
                                        client_name: client.client_name.clone(),
                                        scope: client.scope.clone(),
                                        first_authorized_at: client.first_authorized_at,
                                        last_used_at: client.last_used_at,
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
fn OAuthClientRow(
    client_id: String,
    client_name: Option<String>,
    scope: Option<String>,
    first_authorized_at: chrono::DateTime<chrono::Utc>,
    last_used_at: chrono::DateTime<chrono::Utc>,
) -> Element {
    let oauth2_client_service = use_coroutine_handle::<OAuth2ClientCommand>();
    let display_name = client_name
        .clone()
        .unwrap_or_else(|| format!("{}...", &client_id[..client_id.len().min(8)]));
    let scope_display = scope.clone().unwrap_or_default();

    rsx! {
        tr {
            td {
                p { class: "truncate", "{display_name}" }
            }
            td {
                p { class: "truncate", "{scope_display}" }
            }
            td { r#"{first_authorized_at.date_naive().format("%Y-%m-%d")}"# }
            td { r#"{last_used_at.date_naive().format("%Y-%m-%d")}"# }
            td {
                class: "flex gap-2 justify-center items-center h-8 my-2",
                button {
                    class: "btn btn-sm btn-error hidden sm:block",
                    onclick: {
                        let client_id = client_id.clone();
                        move |_| {
                            oauth2_client_service.send(OAuth2ClientCommand::RevokeClient(client_id.clone()));
                        }
                    },
                    "Revoke"
                }
                button {
                    class: "btn btn-sm btn-error sm:hidden",
                    onclick: {
                        let client_id = client_id.clone();
                        move |_| {
                            oauth2_client_service.send(OAuth2ClientCommand::RevokeClient(client_id.clone()));
                        }
                    },
                    Icon { class: "w-4 h-4", icon: BsTrash }
                }
            }
        }
    }
}
