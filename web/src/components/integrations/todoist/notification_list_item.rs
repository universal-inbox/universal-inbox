#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsCardChecklist};

use universal_inbox::{
    HasHtmlUrl,
    notification::NotificationWithTask,
    third_party::integrations::todoist::{TodoistItem, TodoistItemPriority},
};

use crate::{
    components::{
        Tag, TagDisplay,
        integrations::todoist::{icons::Todoist, list_item::TodoistListItemSubtitle},
        list::{ListContext, ListItem},
        notifications_list::{TaskHint, get_notification_list_item_action_buttons},
    },
    utils::format_elapsed_time,
};

#[component]
pub fn TodoistNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    todoist_item: ReadOnlySignal<TodoistItem>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let task_icon_style = match todoist_item().priority {
        TodoistItemPriority::P1 => "",
        TodoistItemPriority::P2 => "text-yellow-500",
        TodoistItemPriority::P3 => "text-orange-500",
        TodoistItemPriority::P4 => "text-red-500",
    };
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { TodoistListItemSubtitle { todoist_item } },
            link,
            icon: rsx! {
                Todoist { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! {
                Icon { class: "h-5 w-5 min-w-5 {task_icon_style}", icon: BsCardChecklist }
            },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            for tag in todoist_item()
                .labels
                .iter()
                .map(|label| Into::<Tag>::into(label.clone())) {
                    TagDisplay { tag }
                }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}
