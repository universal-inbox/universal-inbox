use std::collections::HashMap;

use dioxus::prelude::*;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus, IntegrationProviderKind,
    },
    IntegrationProviderConfig,
};

use crate::components::icons::{github, todoist};

#[inline_props]
pub fn integrations_panel<'a>(
    cx: Scope,
    integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderConfig>,
    integration_connections: Vec<IntegrationConnection>,
    on_connect: EventHandler<'a, (IntegrationProviderKind, Option<&'a IntegrationConnection>)>,
    on_disconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_reconnect: EventHandler<'a, &'a IntegrationConnection>,
) -> Element {
    cx.render(rsx!(
        div {
            class: "flex flex-col w-auto p-8",

            for (kind, config) in &*integration_providers {
                integration_settings {
                    kind: *kind,
                    config: config.clone(),
                    connection: integration_connections.iter().find(|c| c.provider_kind == *kind).cloned(),
                    on_connect: |c| on_connect.call((*kind, c)),
                    on_disconnect: |c| on_disconnect.call(c),
                    on_reconnect: |c| on_reconnect.call(c),
                }
                div { class: "divider" }
            }
        }
    ))
}

#[inline_props]
pub fn integration_settings<'a>(
    cx: Scope,
    kind: IntegrationProviderKind,
    config: IntegrationProviderConfig,
    connection: Option<Option<IntegrationConnection>>,
    on_connect: EventHandler<'a, Option<&'a IntegrationConnection>>,
    on_disconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_reconnect: EventHandler<'a, &'a IntegrationConnection>,
) -> Element {
    let icon = match kind {
        IntegrationProviderKind::Github => rsx!(self::github { class: "w-8 h-8" }),
        IntegrationProviderKind::Todoist => rsx!(self::todoist { class: "w-8 h-8" }),
    };

    let connection_button_label =
        use_memo(cx, &connection.clone(), |connection| match connection {
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                ..
            })) => "Disconnect",
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Failing,
                ..
            })) => "Reconnect",
            _ => "Connect",
        });
    let connection_button_style =
        use_memo(cx, &connection.clone(), |connection| match connection {
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                ..
            })) => "btn-success",
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Failing,
                ..
            })) => "btn-error",
            _ => "btn-primary",
        });

    cx.render(rsx!(
        div {
            class: "card w-full bg-neutral text-neutral-content",

            div {
                class: "card-body",

                div {
                    class: "flex flex-row",

                    div {
                        class: "card-title",
                        figure { class: "p-2", icon }
                        "{config.name}"
                    }
                    div {
                        class: "flex grow items-center",
                        span {}
                    }
                    div {
                        class: "card-actions justify-end",

                        button {
                            class: "btn {connection_button_style}",
                            onclick: move |_| {
                                match connection {
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Validated, .. })) => on_disconnect.call(c),
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Failing, .. })) => on_reconnect.call(c),
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Created, .. })) => on_connect.call(Some(c)),
                                    _ => on_connect.call(None),
                                }
                            },

                            "{connection_button_label}"
                        }
                    }
                }

                if let Some(Some(IntegrationConnection { failure_message: Some(failure_message), .. })) = connection {
                    rsx!(
                        div {
                            class: "alert alert-error shadow-lg",

                            span { "{failure_message}" }
                        }
                    )
                }
                if let Some(comment) = &config.comment {
                    rsx!(
                        div {
                            class: "alert alert-warning shadow-lg",

                            span { "{comment}" }
                        }
                    )
                }
            }
        }
    ))
}
