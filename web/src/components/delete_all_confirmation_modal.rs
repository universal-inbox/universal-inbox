#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsExclamationTriangle, BsTrash},
};
use gloo_timers::future::TimeoutFuture;

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
                            class: "modal-title flex items-center gap-2",
                            div {
                                class: "badge badge-error badge-soft rounded-full size-8 p-2",
                                Icon {
                                    class: "w-5 h-5 text-error",
                                    icon: BsExclamationTriangle
                                }
                            }
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
                        div {
                            class: "flex items-start gap-3",
                            p {
                                class: "text-sm text-gray-600 dark:text-gray-400",
                                "This action will permanently delete all notifications. Are you sure you want to continue?"
                            }
                        }
                    }

                    div {
                        class: "modal-footer gap-3",
                        button {
                            class: "btn btn-outline",
                            "data-overlay": "#delete-all-confirmation-modal",
                            onclick: move |_| {
                                close_flyonui_modal("#delete-all-confirmation-modal");
                            },
                            "Cancel"
                        }
                        button {
                            class: "btn btn-error",
                            onclick: move |_| {
                                spawn({
                                    async move {
                                        close_flyonui_modal("#delete-all-confirmation-modal");
                                        TimeoutFuture::new(1000).await;
                                        on_confirm.call(());
                                    }
                                });
                            },
                            Icon { class: "w-4 h-4", icon: BsTrash }
                            "Delete All"
                        }
                    }
                }
            }
        }
    }
}
