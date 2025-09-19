#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
pub fn GoogleDrive(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            fill: "currentColor",
            "viewBox": "0 0 50 50",

            title { "Google Drive" }
            path {
                d: "M45.58 31H32.61L19.73 6h10.754c.726 0 1.394.393 1.747 1.027L45.58 31zM23.37 17.43L9.94 43.2 3.482 33.04c-.395-.622-.417-1.411-.055-2.053L17.48 6 23.37 17.43zM45.54 33l-6.401 10.073C38.772 43.65 38.136 44 37.451 44H11.78l5.73-11H45.54z"
            }
        }
    }
}

#[component]
pub fn GoogleDriveFile(mime_type: String, class: Option<String>) -> Element {
    match mime_type.as_str() {
        "application/vnd.google-apps.document" => rsx! { GoogleDriveDocument { class } },
        "application/vnd.google-apps.spreadsheet" => rsx! { GoogleDriveSpreadsheet { class } },
        "application/vnd.google-apps.presentation" => rsx! { GoogleDrivePresentation { class } },
        _ => rsx! { GoogleDriveDocument { class } },
    }
}

#[component]
pub fn GoogleDriveDocument(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            "viewBox": "0 0 50 50",
            fill: "currentColor",
            path {
                d: "M 41.707031 13.792969 L 30.207031 2.292969 C 30.019531 2.105469 29.765625 2 29.5 2 L 11.492188 2 C 9.566406 2 8 3.5625 8 5.480469 L 8 43.902344 C 8 46.160156 9.84375 48 12.113281 48 L 37.886719 48 C 40.15625 48 42 46.160156 42 43.902344 L 42 14.5 C 42 14.234375 41.894531 13.980469 41.707031 13.792969 Z M 26 38 L 17 38 L 17 36 L 26 36 Z M 33 34 L 17 34 L 17 32 L 33 32 Z M 33 30 L 17 30 L 17 28 L 33 28 Z M 33 26 L 17 26 L 17 24 L 33 24 Z M 31.667969 14 C 30.746094 14 30 13.253906 30 12.332031 L 30 4.914063 L 39.085938 14 Z"
            }
        }
    }
}

#[component]
pub fn GoogleDriveSpreadsheet(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            "viewBox": "0 0 50 50",
            fill: "currentColor",

            path {
                d: "M 11.5 2 C 9.574219 2 8 3.550781 8 5.46875 L 8 43.90625 C 8 46.167969 9.855469 48 12.125 48 L 37.875 48 C 40.144531 48 42 46.167969 42 43.90625 L 42 14.5 C 42 14.234375 41.90625 13.96875 41.71875 13.78125 L 30.21875 2.28125 C 30.03125 2.09375 29.765625 2 29.5 2 Z M 30 4.90625 L 39.09375 14 L 31.65625 14 C 30.738281 14 30 13.261719 30 12.34375 Z M 17 24 L 33 24 L 33 38 L 17 38 Z M 19 26 L 19 28 L 24 28 L 24 26 Z M 26 26 L 26 28 L 31 28 L 31 26 Z M 19 30 L 19 32 L 24 32 L 24 30 Z M 26 30 L 26 32 L 31 32 L 31 30 Z M 19 34 L 19 36 L 24 36 L 24 34 Z M 26 34 L 26 36 L 31 36 L 31 34 Z"
            }
        }
    }
}

#[component]
pub fn GoogleDrivePresentation(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            "viewBox": "0 0 50 50",
            fill: "currentColor",
            path {
                d: "M 11.5 2 C 9.574219 2 8 3.550781 8 5.46875 L 8 43.90625 C 8 46.167969 9.855469 48 12.125 48 L 37.875 48 C 40.144531 48 42 46.167969 42 43.90625 L 42 14.5 C 42 14.234375 41.90625 13.96875 41.71875 13.78125 L 30.21875 2.28125 C 30.03125 2.09375 29.765625 2 29.5 2 Z M 30 4.90625 L 39.09375 14 L 31.65625 14 C 30.738281 14 30 13.261719 30 12.34375 Z M 18.9375 23 L 31.0625 23 C 32.132813 23 33 23.867188 33 24.9375 L 33 26 L 17 26 L 17 24.9375 C 17 23.867188 17.867188 23 18.9375 23 Z M 17 28 L 33 28 L 33 34 L 17 34 Z M 17 36 L 33 36 L 33 37.0625 C 33 38.132813 32.132813 39 31.0625 39 L 18.9375 39 C 17.867188 39 17 38.132813 17 37.0625 Z"
            }
        }
    }
}
