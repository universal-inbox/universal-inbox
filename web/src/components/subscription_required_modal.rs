#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use dioxus_free_icons::{Icon, icons::bs_icons::BsExclamationTriangle};

use crate::{route::Route, services::flyonui::init_flyonui_modal};

pub const SUBSCRIPTION_REQUIRED_MODAL_ID: &str = "subscription-required-modal";

#[component]
pub fn SubscriptionRequiredModal() -> Element {
    rsx! {
        div {
            id: "{SUBSCRIPTION_REQUIRED_MODAL_ID}",
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
                                class: "badge badge-warning badge-soft rounded-full size-8 p-2",
                                Icon {
                                    class: "w-5 h-5 text-warning",
                                    icon: BsExclamationTriangle
                                }
                            }
                            "Subscription Required"
                        }
                        button {
                            r#type: "button",
                            class: "btn btn-text btn-circle btn-sm absolute end-3 top-3",
                            "aria-label": "Close",
                            "data-overlay": "#{SUBSCRIPTION_REQUIRED_MODAL_ID}",
                            span { class: "icon-[tabler--x] size-4" }
                        }
                    }

                    div {
                        class: "modal-body pt-4 pb-6",
                        div {
                            class: "flex flex-col gap-4",
                            p {
                                class: "text-sm text-gray-600 dark:text-gray-400",
                                "Your account is in read-only mode. To perform this action, you need an active subscription."
                            }
                            p {
                                class: "text-sm text-gray-600 dark:text-gray-400",
                                "Subscribe now to regain full access to all features."
                            }
                        }
                    }

                    div {
                        class: "modal-footer gap-3",
                        button {
                            class: "btn btn-outline",
                            "data-overlay": "#{SUBSCRIPTION_REQUIRED_MODAL_ID}",
                            "Close"
                        }
                        Link {
                            class: "btn btn-warning",
                            to: Route::SubscriptionSettingsPage {},
                            "data-overlay": "#{SUBSCRIPTION_REQUIRED_MODAL_ID}",
                            "Subscribe Now"
                        }
                    }
                }
            }
        }
    }
}
