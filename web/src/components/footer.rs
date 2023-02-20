use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowDownShort, BsArrowUpShort, BsKeyboard, BsQuestionCircle},
    Icon,
};
use fermi::use_atom_ref;

use crate::services::notification_service::{NOTIFICATIONS, UI_MODEL};

pub fn footer(cx: Scope) -> Element {
    let notifications_ref = use_atom_ref(cx, NOTIFICATIONS);
    let notifications = notifications_ref.read();

    let ui_model_ref = use_atom_ref(cx, UI_MODEL);
    let ui_model = ui_model_ref.read();
    let selected_notification_index = ui_model.selected_notification_index;

    let is_selected_notification_built_from_task = use_memo(
        cx,
        &(selected_notification_index, notifications.clone()),
        |(selected_notification_index, notifications)| {
            let selected_notification = notifications.get(selected_notification_index);
            selected_notification
                .map(|notif| notif.is_built_from_task())
                .unwrap_or(false)
        },
    );

    cx.render(rsx! {
        div {
            class: "w-full drop-shadow-lg",
            hr {}
            button {
                class: "flex w-full items-center h-5",
                onclick: |_| {
                    let mut ui_model = ui_model_ref.write();
                    ui_model.footer_help_opened = !ui_model.footer_help_opened;
                },

                if ui_model.footer_help_opened {
                    rsx! {
                        Icon { class: "w-3 h-3", icon: BsArrowDownShort }
                        div {
                            class: "grow flex items-center justify-center btn btn-ghost h-auto min-h-fit",
                            title: "Help",
                            Icon { class: "w-3 h-3", icon: BsQuestionCircle }
                        }
                        Icon { class: "w-3 h-3", icon: BsArrowDownShort }
                    }
                } else {
                    rsx! {
                        Icon { class: "w-3 h-3", icon: BsArrowUpShort }
                        div {
                            class: "grow flex items-center justify-center btn btn-ghost h-auto min-h-fit",
                            title: "Help",
                            Icon { class: "w-3 h-3", icon: BsQuestionCircle }
                        }
                        Icon { class: "w-3 h-3", icon: BsArrowUpShort }
                    }
                }
            }
            ui_model.footer_help_opened.then(|| rsx! {
                div {
                    class: "flex flex-col px-2 pb-2 text-xs gap-2",

                    div {
                        class: "flex items-center gap-2 pb-2",

                        Icon { class: "w-4 h-4", icon: BsKeyboard }
                        div { "Keyboard shortcuts" }
                    }
                    div {
                        class: "grid grid-cols-4",

                        self::shortcut_text { shortcut: "h", text: "help" }
                        (!is_selected_notification_built_from_task).then(|| rsx!(
                            self::shortcut_text { shortcut: "▼", text: "next notification" }
                            self::shortcut_text { shortcut: "▲", text: "previous notification" }
                            self::shortcut_text { shortcut: "d", text: "delete notification" }
                            self::shortcut_text { shortcut: "u", text: "unsubscribe from notification" }
                            self::shortcut_text { shortcut: "s", text: "snooze notification" }
                            self::shortcut_text { shortcut: "t", text: "add notification to todo task" }
                        )),

                        (is_selected_notification_built_from_task).then(|| rsx!(
                            self::shortcut_text { shortcut: "▼", text: "next task" }
                            self::shortcut_text { shortcut: "▲", text: "previous task" }
                            self::shortcut_text { shortcut: "d", text: "delete task" }
                            self::shortcut_text { shortcut: "c", text: "complete task" }
                            self::shortcut_text { shortcut: "s", text: "snooze notification" }
                        )),
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

            kbd { class: "kbd kbd-sm", "{shortcut}" }
            span { "{text}" }
        }
    })
}
