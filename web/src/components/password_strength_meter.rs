#![allow(non_snake_case)]

use dioxus::prelude::*;
use zxcvbn::zxcvbn;

/// Strength meter shown under a new-password field (ASVS 2.1.8). The score is
/// only a hint: the server enforces the password policy on its own.
#[component]
pub fn PasswordStrengthMeter(
    value: ReadSignal<String>,
    /// Values the password should not be built from (e.g. the user's email).
    #[props(default)]
    user_inputs: Vec<String>,
) -> Element {
    let password = value();
    if password.is_empty() {
        return rsx! {};
    }

    let user_inputs: Vec<&str> = user_inputs.iter().map(String::as_str).collect();
    let entropy = zxcvbn(&password, &user_inputs);
    let score: u8 = entropy.score().into();
    let (label, bar_class, text_class) = match score {
        0 | 1 => ("Weak", "bg-ui-error", "text-ui-error-text"),
        2 => ("Fair", "bg-ui-warning", "text-ui-warning-text"),
        3 => ("Good", "bg-ui-success", "text-ui-success-text"),
        _ => ("Strong", "bg-ui-success", "text-ui-success-text"),
    };
    let feedback = entropy.feedback().map(|feedback| {
        feedback
            .warning()
            .map(|warning| warning.to_string())
            .or_else(|| {
                feedback
                    .suggestions()
                    .first()
                    .map(|suggestion| suggestion.to_string())
            })
    });

    rsx! {
        div { class: "flex flex-col gap-1", "aria-live": "polite",
            div { class: "grid grid-cols-4 gap-1",
                for segment in 1..=4u8 {
                    div {
                        key: "{segment}",
                        class: if segment <= score.max(1) { "h-1 rounded-ui-sm {bar_class}" } else { "h-1 rounded-ui-sm bg-ui-border" },
                    }
                }
            }
            div { class: "text-xs leading-snug",
                span { class: "font-semibold {text_class}", "Password strength: {label}" }
                if let Some(Some(feedback)) = feedback {
                    span { class: "text-ui-base-muted", " · {feedback}" }
                }
            }
        }
    }
}
