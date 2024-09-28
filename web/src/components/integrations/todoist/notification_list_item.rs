#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsCardChecklist, Icon};

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::todoist::{TodoistItem, TodoistItemPriority},
};

use crate::components::{
    integrations::todoist::{icons::Todoist, list_item::TodoistListItemSubtitle},
    list::{ListContext, ListItem},
    notifications_list::{get_notification_list_item_action_buttons, TaskHint},
    Tag, TagDisplay,
};

#[component]
pub fn TodoistNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    todoist_item: ReadOnlySignal<TodoistItem>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notification().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let task_icon_style = match todoist_item().priority {
        TodoistItemPriority::P1 => "",
        TodoistItemPriority::P2 => "text-yellow-500",
        TodoistItemPriority::P3 => "text-orange-500",
        TodoistItemPriority::P4 => "text-red-500",
    };

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { TodoistListItemSubtitle { todoist_item } },
            icon: rsx! { Todoist { class: "h-5 w-5" }, TaskHint { task: notification().task } },
            subicon: rsx! {
                Icon { class: "h-5 w-5 min-w-5 {task_icon_style}", icon: BsCardChecklist }
            },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut
            ),
            is_selected,
            on_select,

            for tag in todoist_item()
                .labels
                .iter()
                .map(|label| Into::<Tag>::into(label.clone())) {
                    TagDisplay { tag }
                }

            span { class: "text-gray-400 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}
