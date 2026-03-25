#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use gloo_timers::future::TimeoutFuture;

use crate::components::ui::button::{Button, ButtonVariant};
use crate::services::flyonui::{close_flyonui_modal, init_flyonui_modal};

#[component]
pub fn DeleteAllConfirmationModal(on_confirm: EventHandler<()>) -> Element {
    rsx! {
        div {
            id: "delete-all-confirmation-modal",
            class: "overlay modal overlay-open:opacity-100 hidden overlay-open:duration-300",
            role: "dialog",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_modal(&web_element);
            },

            div {
                class: "modal-dialog overlay-open:opacity-100 overlay-open:duration-300",
                div {
                    class: "modal-content",

                    div {
                        class: "modal-header",
                        h3 {
                            class: "text-sm font-semibold mb-3 flex items-center gap-2",
                            span { class: "icon-[lucide--triangle-alert] size-5", style: "color: var(--ui-error);" }
                            "Confirm Delete All"
                        }
                        button {
                            r#type: "button",
                            class: "btn btn-text btn-circle btn-sm absolute end-3 top-3",
                            "aria-label": "Close",
                            "data-overlay": "#delete-all-confirmation-modal",
                            span { class: "icon-[tabler--x] size-4" }
                        }
                    }

                    div {
                        class: "modal-body pt-4 pb-6",
                        p {
                            class: "text-xs text-ui-base-muted leading-normal mb-4",
                            "This action will permanently delete all notifications. Are you sure you want to continue?"
                        }
                    }

                    div {
                        class: "modal-footer",
                        div {
                            class: "flex gap-2 justify-end",
                            Button {
                                variant: ButtonVariant::Ghost,
                                onclick: move |_| {
                                    close_flyonui_modal("#delete-all-confirmation-modal");
                                },
                                "Cancel"
                            }
                            Button {
                                variant: ButtonVariant::Danger,
                                icon_class: "icon-[lucide--trash-2]".to_string(),
                                onclick: move |_| {
                                    spawn({
                                        async move {
                                            close_flyonui_modal("#delete-all-confirmation-modal");
                                            TimeoutFuture::new(1000).await;
                                            on_confirm.call(());
                                        }
                                    });
                                },
                                "Delete all"
                            }
                        }
                    }
                }
            }
        }
    }
}
