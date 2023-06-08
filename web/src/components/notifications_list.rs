use dioxus::{events::MouseEvent, prelude::*};
use dioxus_free_icons::{
    icons::bs_icons::{BsBellSlash, BsBookmark, BsCheck2, BsClockHistory, BsLink45deg, BsTrash},
    Icon,
};
use fermi::UseAtomRef;

use universal_inbox::notification::{NotificationMetadata, NotificationWithTask};

use crate::{
    components::icons::{github, todoist},
    model::UniversalInboxUIModel,
};

#[inline_props]
pub fn notifications_list<'a>(
    cx: Scope,
    notifications: Vec<NotificationWithTask>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_delete: EventHandler<'a, &'a NotificationWithTask>,
    on_unsubscribe: EventHandler<'a, &'a NotificationWithTask>,
    on_snooze: EventHandler<'a, &'a NotificationWithTask>,
    on_complete_task: EventHandler<'a, &'a NotificationWithTask>,
    on_plan: EventHandler<'a, &'a NotificationWithTask>,
    on_associate: EventHandler<'a, &'a NotificationWithTask>,
) -> Element {
    let selected_notification_index = ui_model_ref.read().selected_notification_index;

    cx.render(rsx!(table {
        class: "table w-full h-max-full",

        tbody {
            notifications.iter().enumerate().map(|(i, notif)| {
                rsx!{
                    (!notif.is_built_from_task()).then(|| rsx!(
                        self::notification {
                            notif: notif,
                            selected: i == selected_notification_index,
                            ui_model_ref: ui_model_ref,

                            self::notification_button {
                                title: "Delete notification",
                                onclick: |_| on_delete.call(notif),
                                Icon { class: "w-5 h-5" icon: BsTrash }
                            }

                            self::notification_button {
                                title: "Unsubscribe from the notification",
                                onclick: |_| on_unsubscribe.call(notif),
                                Icon { class: "w-5 h-5" icon: BsBellSlash }
                            }

                            self::notification_button {
                                title: "Snooze notification",
                                onclick: |_| on_snooze.call(notif),
                                Icon { class: "w-5 h-5" icon: BsClockHistory }
                            }

                            self::notification_button {
                                title: "Create task",
                                onclick: |_| on_plan.call(notif),
                                Icon { class: "w-5 h-5" icon: BsBookmark }
                            }

                            self::notification_button {
                                title: "Associate to task",
                                onclick: |_| on_associate.call(notif),
                                Icon { class: "w-5 h-5" icon: BsLink45deg }
                            }
                        }
                    )),

                    (notif.is_built_from_task()).then(|| rsx!(
                        self::notification {
                            notif: notif,
                            selected: i == selected_notification_index,
                            ui_model_ref: ui_model_ref,

                            self::notification_button {
                                title: "Delete task",
                                onclick: |_| on_delete.call(notif),
                                Icon { class: "w-5 h-5" icon: BsTrash }
                            }

                            self::notification_button {
                                title: "Complete task",
                                onclick: |_| on_complete_task.call(notif),
                                Icon { class: "w-5 h-5" icon: BsCheck2 }
                            }

                            self::notification_button {
                                title: "Snooze notification",
                                onclick: |_| on_snooze.call(notif),
                                Icon { class: "w-5 h-5" icon: BsClockHistory }
                            }

                            self::notification_button {
                                title: "Plan task",
                                onclick: |_| on_plan.call(notif),
                                Icon { class: "w-5 h-5" icon: BsBookmark }
                            }
                        }
                    ))
                }
            })
        }
    }))
}

#[inline_props]
fn notification<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    selected: bool,
    ui_model_ref: &'a UseAtomRef<UniversalInboxUIModel>,
    children: Element<'a>,
) -> Element {
    let style = use_memo(
        cx,
        (selected,),
        |(selected,)| {
            if selected {
                "active"
            } else {
                ""
            }
        },
    );
    let button_style = use_memo(cx, (selected,), |(selected,)| {
        if selected {
            "visible"
        } else {
            "invisible group-hover:visible"
        }
    });

    cx.render(rsx!(
        tr {
            class: "hover py-1 {style} group snap-start",
            key: "{notif.id}",
            onmousemove: |_| {
                if ui_model_ref.write_silent().set_unhover_element(false) {
                    cx.needs_update();
                }
            },

            self::notification_display { notif: notif }

            td {
                class: "p-2 rounded-none",
                div {
                    class: "{button_style} flex justify-end",
                    children
                }
            }
        }
    ))
}

#[inline_props]
fn notification_display<'a>(cx: Scope, notif: &'a NotificationWithTask) -> Element {
    let icon = match notif.metadata {
        NotificationMetadata::Github(_) => cx.render(rsx!(self::github { class: "h-5 w-5" })),
        NotificationMetadata::Todoist => cx.render(rsx!(self::todoist { class: "h-5 w-5" })),
    };

    cx.render(rsx!(
        td {
             class: "p-2 rounded-none",
             div { class: "flex justify-center", icon } }
        td {
            class: "p-2",

            if let Some(link) = &notif.source_html_url {
                rsx!(a { href: "{link}", target: "_blank", "{notif.title}" })
            } else {
                rsx!("{notif.title}")
            }
        }
    ))
}

#[derive(Props)]
struct NotificationButtonProps<'a> {
    children: Element<'a>,
    title: &'a str,
    #[props(optional)]
    onclick: Option<EventHandler<'a, MouseEvent>>,
}

fn notification_button<'a>(cx: Scope<'a, NotificationButtonProps<'a>>) -> Element {
    cx.render(rsx!(
        button {
            class: "btn btn-ghost btn-square",
            title: "{cx.props.title}",
            onclick: move |evt| {
                if let Some(handler) = &cx.props.onclick {
                    handler.call(evt)
                }
            },

            &cx.props.children
        }
    ))
}
