#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    HasHtmlUrl,
    task::{Task, TaskId, TaskSourceKind},
    third_party::item::{ThirdPartyItemData, ThirdPartyItemKind, ThirdPartyItemSource},
};

use crate::{
    components::{
        integrations::{
            icons::TaskIcon, linear::preview::issue::LinearIssuePreview,
            slack::preview::slack_reaction::SlackReactionTaskPreview,
            ticktick::preview::TickTickTaskPreview, todoist::preview::TodoistTaskPreview,
        },
        markdown::Markdown,
        notification_preview::{
            DETAIL_BODY_INNER, DETAIL_KBD, SOURCE_PILL_ACTIVE, SOURCE_PILL_BASE, SOURCE_PILL_SUB,
            SOURCE_PILL_TILE,
        },
        tasks_list::TaskListContext,
        ui::{ActionButton, Button, ButtonVariant},
    },
    model::UniversalInboxUIModel,
    services::task_service::TaskCommand,
    utils::reset_scroll_top,
};

#[component]
pub fn TaskPreview(
    ui_model: Signal<UniversalInboxUIModel>,
    task: ReadSignal<Task>,
    expand_details: ReadSignal<bool>,
    is_help_enabled: ReadSignal<bool>,
    tasks_count: ReadSignal<usize>,
) -> Element {
    let task_service = use_coroutine_handle::<TaskCommand>();
    let context = use_memo(move || TaskListContext {
        is_task_actions_enabled: ui_model.read().is_task_actions_enabled,
        task_service,
    });
    use_context_provider(move || context);
    let shortcut_visibility_style = use_memo(move || {
        if is_help_enabled() {
            "visible"
        } else {
            "invisible"
        }
    });
    let is_first = ui_model.read().selected_task_index.unwrap_or_default() == 0;
    let is_last = ui_model.read().selected_task_index.unwrap_or_default() == tasks_count() - 1;
    let task_type = task_sub_type(&task());

    let mut latest_shown_task_id = use_signal(|| None::<TaskId>);
    use_effect(move || {
        // reset scroll position when showing another task
        let mut latest = latest_shown_task_id.write();
        if *latest != Some(task().id) {
            *latest = Some(task().id);
            let _ = reset_scroll_top("task-preview-details");
        }
    });

    let task_pill_class = format!("{SOURCE_PILL_BASE} {SOURCE_PILL_ACTIVE}");

    rsx! {
        // Detail header: back button (mobile) + tab on the left, actions on the right
        div {
            class: "py-1.5 px-5 bg-ui-surface border-b border-ui-border flex items-center justify-between gap-2 shrink-0",

            // Back button for mobile — baseline `hidden`,
            // `max-md:[.app-layout.show-detail_&]:inline-flex!` reveals it on
            // the mobile detail pane. The trailing `!` (`!important`) wins
            // against `hidden`.
            Button {
                variant: ButtonVariant::Ghost,
                class: "hidden max-md:[.app-layout.show-detail_&]:inline-flex!".to_string(),
                aria_label: "Back to list".to_string(),
                title: "Back to list".to_string(),
                onclick: move |_| ui_model.write().selected_task_index = None,
                icon_class: "icon-[tabler--arrow-left]".to_string(),
            }

            div {
                class: "inline-flex items-center gap-1",
                button {
                    class: "{task_pill_class}",
                    role: "tab",
                    "aria-pressed": "true",
                    span { class: SOURCE_PILL_TILE,
                        TaskIcon { class: "h-3 w-3".to_string(), kind: task().kind }
                    }
                    span { "{task_source_display_name(task().kind)}" }
                    span { class: SOURCE_PILL_SUB, "· {task_type}" }
                }
            }

            div {
                class: "flex items-center gap-1.5 ml-auto",

                if shortcut_visibility_style == "visible" {
                    span { class: DETAIL_KBD, "e" }
                }

                // Open in source button — common to every task kind
                Button {
                    variant: ButtonVariant::Ghost,
                    href: task().get_html_url().to_string(),
                    aria_label: format!("Open in {}", task_source_display_name(task().kind)),
                    title: format!("Open in {}", task_source_display_name(task().kind)),
                    icon_class: "icon-[lucide--external-link]".to_string(),
                    enable_tooltip: true,
                }
            }
        }

        div {
            class: "flex-1 overflow-hidden py-3 px-5 min-h-0 flex flex-col animate-[detail-fade_0.2s_var(--ui-ease-out)]",
            div {
                class: DETAIL_BODY_INNER,
                TaskDetailsPreview { task, expand_details }
            }
        }

        // Detail dock: bottom action bar
        div {
            class: "py-1.5 px-5 bg-ui-surface border-t border-ui-border flex items-center justify-between shrink-0",

            div {
                class: "inline-flex items-center gap-1 text-ui-base-muted",
                Button {
                    variant: ButtonVariant::Icon,
                    disabled: is_first,
                    aria_label: "Previous task".to_string(),
                    onclick: move |_| {
                        let mut model = ui_model.write();
                        model.selected_task_index = Some(model.selected_task_index.unwrap_or_default() - 1);
                    },
                    icon_class: "icon-[tabler--chevron-left]".to_string(),
                }

                span { class: "text-[11px] font-medium text-ui-base-muted tabular-nums", "{ui_model.read().selected_task_index.unwrap_or_default() + 1} / {tasks_count()}" }

                Button {
                    variant: ButtonVariant::Icon,
                    disabled: is_last,
                    aria_label: "Next task".to_string(),
                    onclick: move |_| {
                        let mut model = ui_model.write();
                        model.selected_task_index = Some(model.selected_task_index.unwrap_or_default() + 1);
                    },
                    icon_class: "icon-[tabler--chevron-right]".to_string(),
                }
            }

            div {
                class: "flex items-center gap-1.5 min-w-0",
                for btn in get_task_action_buttons(
                    task,
                    shortcut_visibility_style == "visible") {
                    { btn }
                }
            }
        }
    }
}

#[component]
pub fn TaskDetailsPreview(task: ReadSignal<Task>, expand_details: ReadSignal<bool>) -> Element {
    match task().source_item.data {
        ThirdPartyItemData::TickTickItem(ticktick_item) => rsx! {
            TickTickTaskPreview { ticktick_item: *ticktick_item, task }
        },
        ThirdPartyItemData::TodoistItem(todoist_item) => rsx! {
            TodoistTaskPreview { todoist_item: *todoist_item, task }
        },
        ThirdPartyItemData::SlackReaction(slack_reaction) => rsx! {
            SlackReactionTaskPreview { slack_reaction: *slack_reaction, task }
        },
        ThirdPartyItemData::LinearIssue(linear_issue) => {
            // Show the issue's own title, so the name reads the same as it does
            // in Linear, and hand any renamed task-manager title to the
            // subtitle so both names stay visible.
            let source_title = linear_issue.render_task_title();
            let subtitle = task_manager_title_subtitle(&task(), &source_title);
            rsx! {
                LinearIssuePreview {
                    title: source_title,
                    subtitle,
                    linear_issue: *linear_issue,
                    linear_notification: None,
                    expand_details
                }
            }
        }
        ThirdPartyItemData::SlackThread(_)
        | ThirdPartyItemData::LinearNotification(_)
        | ThirdPartyItemData::GithubNotification(_)
        | ThirdPartyItemData::GoogleMailThread(_)
        | ThirdPartyItemData::GoogleCalendarEvent(_)
        | ThirdPartyItemData::GoogleDriveComment(_)
        | ThirdPartyItemData::WebPage(_) => rsx! {},
    }
}

/// The header subtitle that surfaces the title a task carries in the user's
/// task manager.
///
/// A source-only integration (Linear, Slack) seeds a task's title once and then
/// never re-asserts it, so the stored title diverges from the source's as soon
/// as the user renames the task in Todoist or TickTick. The preview keeps
/// showing the source title next to the source's own brand icon, and this
/// subtitle carries the other name next to the task manager's icon, so both are
/// visible at once. It returns `None` when the two agree, which is the common
/// case and would otherwise print the same string twice.
pub fn task_manager_title_subtitle(task: &Task, source_title: &str) -> Option<Element> {
    if task.title == source_title {
        return None;
    }

    let title = task.title.clone();
    let task_manager_kind = task
        .sink_item
        .as_ref()
        .and_then(|sink_item| {
            TaskSourceKind::try_from(sink_item.get_third_party_item_source_kind()).ok()
        })
        // An unmirrored task has no sink item, yet its title can still have been
        // renamed through the API. Show the name without claiming an owner.
        .map(|kind| {
            rsx! {
                span {
                    class: "shrink-0 inline-flex items-center",
                    "aria-hidden": "true",
                    TaskIcon { class: "h-3.5 w-3.5".to_string(), kind }
                }
            }
        });

    Some(rsx! {
        if let Some(icon) = task_manager_kind {
            { icon }
        }
        Markdown {
            text: "{title}",
            class: "preview-head-subtitle-text".to_string(),
        }
    })
}

pub fn task_source_display_name(kind: TaskSourceKind) -> &'static str {
    match kind {
        TaskSourceKind::Todoist => "Todoist",
        TaskSourceKind::TickTick => "TickTick",
        TaskSourceKind::Slack => "Slack",
        TaskSourceKind::Linear => "Linear",
    }
}

pub fn task_sub_type(task: &Task) -> &'static str {
    match task.source_item.kind() {
        ThirdPartyItemKind::SlackReaction => "Reaction",
        ThirdPartyItemKind::LinearIssue => "Issue",
        ThirdPartyItemKind::TickTickItem
        | ThirdPartyItemKind::TodoistItem
        | ThirdPartyItemKind::SlackThread
        | ThirdPartyItemKind::LinearNotification
        | ThirdPartyItemKind::GithubNotification
        | ThirdPartyItemKind::GoogleMailThread
        | ThirdPartyItemKind::GoogleCalendarEvent
        | ThirdPartyItemKind::GoogleDriveComment
        | ThirdPartyItemKind::WebPage => "Task",
    }
}

pub fn get_task_action_buttons(task: ReadSignal<Task>, show_shortcut: bool) -> Vec<Element> {
    let context = use_context::<Memo<TaskListContext>>();

    vec![rsx! {
        ActionButton {
            title: "Complete task",
            shortcut: "c",
            disabled_label: (!context().is_task_actions_enabled)
                .then_some("No task management service connected".to_string()),
            show_shortcut,
            onclick: move |_| {
                context().task_service
                    .send(TaskCommand::Complete(task().id));
            },
            icon_class: "icon-[lucide--check-circle]"
        }
    }]
}
