#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::error;

use universal_inbox::user::{Password, PasswordResetToken, UserId};

use crate::{
    components::{
        auth_widgets::{Backlink, PrimaryBtn},
        floating_label_inputs::FloatingLabelInputText,
        ui::PageHeader,
    },
    form::FormValues,
    route::Route,
    services::user_service::UserCommand,
};

#[component]
pub fn PasswordUpdatePage(user_id: UserId, password_reset_token: PasswordResetToken) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let password = use_signal(|| "".to_string());
    let mut force_validation = use_signal(|| false);

    rsx! {
        Backlink { to: Route::LoginPage {}, "Back to login" }
        PageHeader {
            title: "Reset your password".to_string(),
            subtitle: Some("Enter your new password below.".to_string()),
        }

        form {
            "novalidate": "true",
            onsubmit: move |evt| {
                evt.prevent_default();
                match FormValues(evt.values()).try_into() {
                    Ok(password) => {
                        user_service.send(UserCommand::ResetPassword(user_id, password_reset_token.clone(), password));
                    },
                    Err(err) => {
                        *force_validation.write() = true;
                        error!("Failed to parse form values as Password: {err}");
                    }
                }
            },

            FloatingLabelInputText::<Password> {
                name: "password".to_string(),
                label: Some("New password".to_string()),
                required: true,
                value: password,
                autofocus: true,
                force_validation: force_validation(),
                r#type: "password".to_string(),
                field_icon_class: "icon-[lucide--lock]".to_string(),
                placeholder: "At least 10 characters".to_string(),
                help: "Use 10+ characters with a mix of letters, numbers and symbols.".to_string(),
            }

            PrimaryBtn { button_type: "submit".to_string(), "Reset password" }
        }

        div { class: "mt-auto pt-6 text-center text-xs text-ui-base-muted",
            "Remembered it? "
            Link {
                class: "text-ui-primary font-semibold no-underline hover:text-ui-primary-hover hover:underline",
                to: Route::LoginPage {},
                "Log in"
            }
        }
    }
}
