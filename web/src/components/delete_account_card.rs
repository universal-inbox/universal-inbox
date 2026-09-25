#![allow(non_snake_case)]

//! "Delete my account" danger zone of the Profile page (GDPR right to
//! erasure). The user re-types their email address (or `DELETE` when the
//! account has no email) before the destructive button is enabled; the API
//! checks the same confirmation (`DELETE /api/users/me`).

use dioxus::prelude::*;

use crate::{
    components::ui::{Button, ButtonVariant, Card, CardBody, CardHeader, CardMeta, CardVariant},
    services::user_service::{CONNECTED_USER, UserCommand},
};

#[component]
pub fn DeleteAccountCard() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let mut confirmation = use_signal(String::new);

    let Some(user) = CONNECTED_USER.read().clone() else {
        return rsx! {};
    };
    let expected_confirmation = user.account_deletion_confirmation();
    let is_confirmed = user.is_account_deletion_confirmed(&confirmation());

    rsx! {
        section {
            role: "region",
            aria_label: "Delete account",

            Card { variant: CardVariant::ApiKeys, id: "delete-account-card".to_string(),
                CardHeader { interactive: false,
                    span { class: "icon-[lucide--triangle-alert] size-5 text-ui-error" }
                    CardMeta { name: "Delete my account" }
                }

                CardBody { class: "px-3.5 pb-3.5 flex flex-col gap-3",
                    p { class: "text-sm text-ui-base-muted leading-normal m-0",
                        "This permanently deletes your Universal Inbox account and all its data: "
                        "notifications, tasks, integration connections, API keys and authorized apps. "
                        "Any paid subscription is cancelled immediately. This cannot be undone."
                    }

                    div {
                        label {
                            class: "block text-xs font-semibold uppercase tracking-wider text-ui-base-muted mb-1",
                            r#for: "deleteAccountConfirmation",
                            "Type {expected_confirmation} to confirm"
                        }
                        input {
                            class: "w-full px-2.5 py-1.5 text-[var(--ui-text-base)] font-ui bg-ui-base-200 text-ui-base-content border border-ui-border rounded-ui-sm outline-none transition-[border-color,box-shadow] duration-150 ease-[var(--ui-ease)] focus:border-ui-error focus:shadow-[var(--ui-focus-ring)] focus:outline-none",
                            id: "deleteAccountConfirmation",
                            name: "confirmation",
                            r#type: "text",
                            autocomplete: "off",
                            value: "{confirmation}",
                            oninput: move |evt| confirmation.set(evt.value()),
                        }
                    }

                    div {
                        Button {
                            id: "delete-account-button".to_string(),
                            variant: ButtonVariant::Danger,
                            icon_class: "icon-[lucide--trash-2]".to_string(),
                            disabled: !is_confirmed,
                            onclick: move |_| {
                                if is_confirmed {
                                    user_service.send(UserCommand::DeleteAccount(confirmation()));
                                }
                            },
                            "Delete my account"
                        }
                    }
                }
            }
        }
    }
}
