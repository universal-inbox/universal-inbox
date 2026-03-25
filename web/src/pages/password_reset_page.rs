#![allow(non_snake_case)]

use dioxus::prelude::*;
use email_address::EmailAddress;
use log::error;

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

pub fn PasswordResetPage() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let email = use_signal(|| "".to_string());
    let mut force_validation = use_signal(|| false);

    rsx! {
        Backlink { to: Route::LoginPage {}, "Back to login" }
        PageHeader {
            title: "Reset your password".to_string(),
            subtitle: Some("Enter the email on your account and we'll send a reset link. The link expires in 30 minutes.".to_string()),
        }

        form {
            "novalidate": "true",
            onsubmit: move |evt| {
                evt.prevent_default();
                match FormValues(evt.values()).try_into() {
                    Ok(email_address) => {
                        user_service.send(UserCommand::SendPasswordResetEmail(email_address));
                    },
                    Err(err) => {
                        *force_validation.write() = true;
                        error!("Failed to parse form values as EmailAddress: {err}");
                    }
                }
            },

            FloatingLabelInputText::<EmailAddress> {
                name: "email".to_string(),
                label: Some("Email".to_string()),
                required: true,
                value: email,
                autofocus: true,
                force_validation: force_validation(),
                r#type: "email".to_string(),
                field_icon_class: "icon-[lucide--mail]".to_string(),
                placeholder: "you@company.com".to_string(),
            }

            PrimaryBtn { button_type: "submit".to_string(), "Send reset link" }
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
