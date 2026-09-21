#![allow(non_snake_case)]
//! Upgrade modal driven by `UPGRADE_TRIGGER`. Mounted once at app level
//! (`NavBarLayout`) and pops whenever any API call sets a trigger. Calling
//! services don't need to know it exists — they just write the trigger.

use dioxus::prelude::*;
use universal_inbox::billing::FREE_PLAN_INTEGRATION_LIMIT_CODE;

use crate::components::ui::button::{Button, ButtonVariant};
use crate::services::billing_service::{
    BILLING_STATE, BillingAvailability, BillingCommand, UPGRADE_TRIGGER, UpgradeTrigger,
    open_checkout,
};

#[component]
pub fn UpgradeModal() -> Element {
    let trigger = UPGRADE_TRIGGER.read().clone();
    let Some(trigger) = trigger else {
        return rsx! { Fragment {} };
    };

    // Skip the modal entirely on self-hosted instances — billing isn't
    // available so there's nothing to upgrade to.
    if BILLING_STATE.read().clone() == BillingAvailability::Disabled {
        // Drop the trigger so we don't keep checking on every render.
        *UPGRADE_TRIGGER.write() = None;
        return rsx! { Fragment {} };
    }

    let billing_service = use_coroutine_handle::<BillingCommand>();

    let (headline, description) = copy_for(&trigger);

    let dismiss = move |_| {
        *UPGRADE_TRIGGER.write() = None;
    };
    let upgrade = move |_| {
        billing_service.send(open_checkout());
        *UPGRADE_TRIGGER.write() = None;
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[2000] flex items-center justify-center bg-black/45 px-4",
            role: "dialog",
            "aria-modal": "true",
            "aria-labelledby": "upgrade-modal-headline",
            onclick: dismiss,
            // The app declares no base font-size, so every text node carries
            // its own size class — without one it renders at the 16px browser
            // default, far above the surrounding UI.
            div {
                class: "max-w-[420px] w-full rounded-ui-md border border-ui-border bg-ui-base-100 \
                        shadow-ui-lg p-5 font-ui",
                onclick: move |e| e.stop_propagation(),

                h2 {
                    id: "upgrade-modal-headline",
                    class: "text-sm font-semibold text-ui-base-content mb-2 flex items-center gap-2",
                    span { class: "icon-[lucide--sparkles] size-4 text-ui-primary" }
                    "{headline}"
                }
                p {
                    class: "text-xs text-ui-base-muted leading-normal mb-4",
                    "{description}"
                }

                div {
                    class: "flex gap-2 justify-end",
                    Button {
                        variant: ButtonVariant::Ghost,
                        onclick: dismiss,
                        "Maybe later"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: upgrade,
                        "Upgrade to Paid"
                    }
                }
            }
        }
    }
}

fn copy_for(trigger: &UpgradeTrigger) -> (String, String) {
    let default_description = trigger
        .message
        .clone()
        .unwrap_or_else(|| "Upgrade to Paid to remove free-plan limits and continue.".to_string());
    match trigger.code.as_str() {
        FREE_PLAN_INTEGRATION_LIMIT_CODE => {
            ("Free plan limit reached".to_string(), default_description)
        }
        _ => ("Upgrade required".to_string(), default_description),
    }
}
