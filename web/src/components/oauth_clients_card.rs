#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::{
    components::{
        loading::Loading,
        ui::{Button, ButtonVariant, Card, CardEmptyState, CardHeader, CardMeta, CardVariant},
    },
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
            Card { variant: CardVariant::ApiKeys,
                Loading { label: "Loading authorized OAuth2 clients..." }
            }
        };
    };

    rsx! {
        section {
            role: "region",
            aria_label: "Authorized OAuth2 clients",

            Card {
                variant: CardVariant::ApiKeys,
                CardHeader {
                    span { class: "icon-[lucide--users] size-5" }
                    CardMeta { name: "Authorized OAuth2 clients" }
                }

                if authorized_clients.is_empty() {
                    CardEmptyState {
                        icon_class: "icon-[lucide--users]".to_string(),
                        title: "No authorized clients".to_string(),
                    }
                } else {
                    table {
                        class: "api-keys-table max-md:block max-md:overflow-x-auto",
                        thead {
                            tr {
                                th { style: "width: 140px;", "Client name" }
                                th { class: "max-md:hidden", style: "width: 70px;", "Scope" }
                                th { class: "max-md:hidden", style: "width: 100px;", "First authorized" }
                                th { class: "max-md:hidden", style: "width: 85px;", "Last used" }
                                th { style: "width: 145px;", aria_label: "Actions", "" }
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

#[component]
fn OAuthClientRow(
    client_id: String,
    client_name: Option<String>,
    scope: Option<String>,
    first_authorized_at: chrono::DateTime<chrono::Utc>,
    last_used_at: chrono::DateTime<chrono::Utc>,
) -> Element {
    let oauth2_client_service = use_coroutine_handle::<OAuth2ClientCommand>();
    let mut confirming = use_signal(|| false);
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
                class: "max-md:hidden",
                p { class: "truncate", "{scope_display}" }
            }
            td { class: "max-md:hidden", r#"{first_authorized_at.date_naive().format("%Y-%m-%d")}"# }
            td { class: "max-md:hidden", r#"{last_used_at.date_naive().format("%Y-%m-%d")}"# }
            td {
                div {
                    class: "flex items-center justify-end gap-1",
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
}
