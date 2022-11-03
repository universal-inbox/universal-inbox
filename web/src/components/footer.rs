use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowDownShort, BsArrowUpShort, BsKeyboard, BsQuestionCircle},
    Icon,
};

use crate::services::notification_service::UI_MODEL;

pub fn footer(cx: Scope) -> Element {
    let ui_model = use_atom_ref(&cx, UI_MODEL);

    cx.render(rsx! {
        div {
            class: "w-full drop-shadow-lg bg-light-0 dark:bg-dark-200",
            hr { class: "text-light-200 dark:text-dark-300" }
            button {
                class: "flex w-full items-center h-5 hover:bg-light-400 hover:dark:bg-dark-600",
                onclick: |_| {
                    let mut model = ui_model.write();
                    model.footer_help_opened = !model.footer_help_opened;
                },

                if ui_model.read().footer_help_opened {
                    rsx! {
                        Icon { class: "w-3 h-3", icon: BsArrowDownShort }
                        div { class: "grow flex items-center justify-center", Icon { class: "w-3 h-3", icon: BsQuestionCircle } }
                        Icon { class: "w-3 h-3", icon: BsArrowDownShort }
                    }
                } else {
                    rsx! {
                        Icon { class: "w-3 h-3", icon: BsArrowUpShort }
                        div { class: "grow flex items-center justify-center", Icon { class: "w-3 h-3", icon: BsQuestionCircle } }
                        Icon { class: "w-3 h-3", icon: BsArrowUpShort }
                    }
                }
            }
            ui_model.read().footer_help_opened.then(|| rsx! {
                div {
                    class: "flex flex-col px-2 pb-2 text-xs text-gray-100",

                    div {
                        class: "flex items-center gap-2",

                        Icon { class: "w-4 h-4", icon: BsKeyboard }
                        div { "Keyboard shortcuts" }
                    }
                    div {
                        class: "grid grid-cols-4 text-slate-500",

                        self::shortcut_text { shortcut: "h", text: "help" }
                        div {
                            class: "flex items-center gap-2",

                            Icon { class: "text-red-500 w-4 h-4", icon: BsArrowDownShort }
                            span { "next notification" }
                        }
                        div {
                            class: "flex items-center gap-2",

                            Icon { class: "text-red-500 w-4 h-4", icon: BsArrowUpShort }
                            span { "previous notification" }
                        }
                        self::shortcut_text { shortcut: "d", text: "mark notification as done" }
                        self::shortcut_text { shortcut: "u", text: "unsubscribe from notification" }
                        self::shortcut_text { shortcut: "s", text: "snooze notification" }
                        self::shortcut_text { shortcut: "t", text: "add notification to todo task" }
                    }
                }
            })
        }
    })
}

#[inline_props]
pub fn shortcut_text<'a>(cx: Scope, text: &'a str, shortcut: &'a str) -> Element {
    cx.render(rsx! {
        div {
            class: "flex items-center gap-2",

            span { class: "text-red-500 flex items-center justify-center w-4", "{shortcut}" }
            span { "{text}" }
        }
    })
}
