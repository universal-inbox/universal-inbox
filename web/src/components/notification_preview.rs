#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    HasHtmlUrl,
    notification::{NotificationId, NotificationSourceKind, NotificationWithTask},
    third_party::{
        integrations::{github::GithubNotificationItem, slack::SlackReactionItem},
        item::ThirdPartyItemData,
    },
};

use crate::{
    components::{
        integrations::{
            api::web_page::preview::WebPagePreview,
            github::preview::{
                GithubNotificationDefaultPreview, discussion::GithubDiscussionPreview,
                pull_request::GithubPullRequestPreview,
            },
            google_calendar::preview::GoogleCalendarEventPreview,
            google_drive::preview::GoogleDriveCommentPreview,
            google_mail::preview::GoogleMailThreadPreview,
            icons::{NotificationIcon, TaskIcon},
            linear::preview::LinearNotificationPreview,
            slack::preview::{
                file::SlackFilePreview, message::SlackMessagePreview, thread::SlackThreadPreview,
            },
        },
        notifications_list::NotificationListContext,
        task_preview::{TaskDetailsPreview, task_source_display_name, task_sub_type},
        ui::{ActionButton, Button, ButtonVariant},
    },
    model::{PreviewPane, UniversalInboxUIModel},
    services::notification_service::NotificationCommand,
    utils::reset_scroll_top,
};

// Shared style strings for the source-pill tab button rendered in the detail
// header (used here for Notification/Task tabs, and in `task_preview.rs` for
// the lone Task tab on the synced-tasks page). Composing `SOURCE_PILL_BASE`
// with one of the state strings produces the full class string.
pub(crate) const SOURCE_PILL_BASE: &str = "inline-flex items-center gap-1.5 pl-1 pr-2.5 py-[3px] rounded-ui-pill bg-ui-surface border text-xs font-semibold font-ui transition-colors duration-[120ms] ease-[var(--ui-ease)]";
pub(crate) const SOURCE_PILL_ACTIVE: &str =
    "border-ui-primary text-ui-base-content shadow-ui-sm cursor-default";
pub(crate) const SOURCE_PILL_INACTIVE: &str = "border-ui-border text-ui-base-muted cursor-pointer hover:border-ui-primary hover:text-ui-base-content";

// Wrapper inside `.detail-body`. `flex-1 min-h-0` replaces the former
// `.detail-body > *` rule (lets the scroll container grow + shrink); the
// `mx-auto w-full max-w-3xl` half caps content at 768px and centers it on
// wide panes.
pub(crate) const DETAIL_BODY_INNER: &str = "flex-1 min-h-0 mx-auto w-full max-w-3xl";

// 20px tile that frames an integration brand icon inside a source pill. The
// `[&>*]:size-3!` / `[&>*]:text-[12px]!` arbitrary variants replace the former
// `.ui-detail-source-tile > *` CSS rule that forced icons (which otherwise
// render at h-5/w-5) down to 12px inside this small frame.
pub(crate) const SOURCE_PILL_TILE: &str = "inline-flex size-5 items-center justify-center rounded-ui-sm bg-ui-surface border border-ui-border shrink-0 [&>*]:size-3! [&>*]:text-[12px]!";

// Sub-label inside a source pill (e.g. "· Pull request"). Was `.sub` in CSS.
pub(crate) const SOURCE_PILL_SUB: &str = "font-medium text-ui-base-muted ml-0.5";

// Small monospace badge for keyboard shortcut hints (e.g. "e", "tab") in the
// detail header / dock action clusters. Was `.detail-kbd` in CSS.
pub(crate) const DETAIL_KBD: &str = "text-[9px] font-mono px-1 py-px rounded-ui-xs bg-ui-base-200 border border-ui-border text-ui-base-muted leading-none";

#[component]
pub fn NotificationPreview(
    ui_model: Signal<UniversalInboxUIModel>,
    notification: ReadSignal<NotificationWithTask>,
    notifications_count: ReadSignal<usize>,
) -> Element {
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let context = use_memo(move || NotificationListContext {
        is_task_actions_enabled: ui_model.read().is_task_actions_enabled,
        notification_service,
    });
    use_context_provider(move || context);
    let has_notification_details_preview = !notification().is_built_from_task();
    let has_task_details_preview = notification().task.is_some();
    let shortcut_visibility_style = use_memo(move || {
        if ui_model.read().is_help_enabled {
            "visible"
        } else {
            "invisible"
        }
    });

    let mut latest_shown_notification_id = use_signal(|| None::<NotificationId>);
    use_effect(move || {
        // reset selected_preview_pane, preview_cards_expanded and scroll position when showing another notification
        let mut latest_shown_notification_id = latest_shown_notification_id.write();
        if *latest_shown_notification_id != Some(notification().id) {
            let mut ui_model = ui_model.write();
            ui_model.selected_preview_pane = if has_notification_details_preview {
                PreviewPane::Notification
            } else {
                PreviewPane::Task
            };
            ui_model.preview_cards_expanded = false;
            *latest_shown_notification_id = Some(notification().id);
            let _ = reset_scroll_top("notification-preview-details");
        }
    });

    let previous_button_style = if ui_model
        .read()
        .selected_notification_index
        .unwrap_or_default()
        == 0
    {
        "disabled"
    } else {
        ""
    };
    let next_button_style = if ui_model
        .read()
        .selected_notification_index
        .unwrap_or_default()
        == notifications_count() - 1
    {
        "disabled"
    } else {
        ""
    };

    let is_notification_active = ui_model.read().selected_preview_pane == PreviewPane::Notification;
    let notification_pill_class = format!(
        "{SOURCE_PILL_BASE} {}",
        if is_notification_active {
            SOURCE_PILL_ACTIVE
        } else {
            SOURCE_PILL_INACTIVE
        }
    );
    let task_pill_class = format!(
        "{SOURCE_PILL_BASE} {}",
        if is_notification_active {
            SOURCE_PILL_INACTIVE
        } else {
            SOURCE_PILL_ACTIVE
        }
    );

    rsx! {
        // Detail header: back button (mobile) + tabs on the left, actions on the right
        div {
            class: "py-1.5 px-5 bg-ui-surface border-b border-ui-border flex items-center justify-between gap-2 shrink-0",

                // Back button for mobile — first on the left, only visible
                // on mobile in detail view. Baseline `hidden` keeps it off;
                // `max-md:[.app-layout.show-detail_&]:inline-flex!` reveals
                // it when the parent layout enters show-detail at ≤768px.
                // The `!` (`!important` in Tailwind v4) wins against `hidden`.
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "hidden max-md:[.app-layout.show-detail_&]:inline-flex!".to_string(),
                    aria_label: "Back to list".to_string(),
                    title: "Back to list".to_string(),
                    onclick: move |_| ui_model.write().selected_notification_index = None,
                    icon_class: "icon-[tabler--arrow-left]".to_string(),
                }

                div {
                    class: "inline-flex items-center gap-1",

                    if has_notification_details_preview {
                        button {
                            class: "{notification_pill_class}",
                            role: "tab",
                            "aria-label": "Show notification preview",
                            "aria-pressed": "{is_notification_active}",
                            onclick: move |_| { ui_model.write().selected_preview_pane = PreviewPane::Notification },
                            span { class: SOURCE_PILL_TILE,
                                NotificationIcon { kind: notification().kind }
                            }
                            span { "{source_display_name(notification().kind)}" }
                            if let Some(sub) = notification_sub_type(&notification().source_item.data) {
                                span { class: SOURCE_PILL_SUB, "· {sub}" }
                            }
                        }
                    }
                    if has_notification_details_preview && has_task_details_preview {
                        // Circular "link" badge between the two tab pills. The two
                        // before:/after: pseudo-elements draw the 4px connector
                        // lines that bridge it to the adjacent pills (these
                        // replace the former `.detail-tabs-link::before/::after`
                        // CSS rules).
                        span {
                            class: "relative inline-flex size-5 items-center justify-center rounded-full border border-ui-border bg-ui-surface text-ui-base-muted shrink-0 before:content-[''] before:absolute before:top-1/2 before:left-[-4px] before:w-1 before:h-px before:bg-ui-border after:content-[''] after:absolute after:top-1/2 after:right-[-4px] after:w-1 after:h-px after:bg-ui-border",
                            "aria-hidden": "true",
                            title: "Linked task",
                            span { class: "icon-[lucide--link-2] size-[11px] text-[11px]" }
                        }
                    }
                    if has_task_details_preview {
                        if let Some(task) = notification().task {
                            button {
                                class: "{task_pill_class}",
                                role: "tab",
                                "aria-label": "Show linked task preview",
                                "aria-pressed": "{!is_notification_active}",
                                onclick: move |_| { ui_model.write().selected_preview_pane = PreviewPane::Task },
                                span { class: SOURCE_PILL_TILE,
                                    TaskIcon { class: "h-3 w-3".to_string(), kind: task.kind }
                                }
                                span { "{task_source_display_name(task.kind)}" }
                                span { class: SOURCE_PILL_SUB, "· {task_sub_type(&task)}" }
                            }
                        }
                    }
                }

                div {
                    class: "flex items-center gap-1.5 ml-auto",

                    if shortcut_visibility_style == "visible" {
                        if has_task_details_preview {
                            span {
                                class: DETAIL_KBD,
                                "tab"
                            }
                        }
                        span {
                            class: DETAIL_KBD,
                            "e"
                        }
                    }

                    // Open in source button — common to every preview kind
                    Button {
                        variant: ButtonVariant::Ghost,
                        href: notification().get_html_url().to_string(),
                        aria_label: format!("Open in {}", source_display_name(notification().kind)),
                        title: format!("Open in {}", source_display_name(notification().kind)),
                        icon_class: "icon-[lucide--external-link]".to_string(),
                        enable_tooltip: true,
                    }
                }
            }

            div {
                class: "flex-1 overflow-hidden py-3 px-5 min-h-0 flex flex-col animate-[detail-fade_0.2s_var(--ui-ease-out)]",

                match ui_model.read().selected_preview_pane {
                    PreviewPane::Notification => rsx! {
                        div {
                            id: "notification-tab",
                            class: DETAIL_BODY_INNER,
                            NotificationDetailsPreview {
                                notification,
                                expand_details: ui_model.read().preview_cards_expanded
                            }
                        }
                    },
                    PreviewPane::Task => rsx! {
                        if let Some(task) = notification().task {
                            div {
                                id: "task-tab",
                                class: DETAIL_BODY_INNER,
                                TaskDetailsPreview {
                                    task,
                                    expand_details: ui_model.read().preview_cards_expanded
                                }
                            }
                        }
                    },
                }
            }

            // Detail dock: bottom action bar
            div {
                class: "py-1.5 px-5 bg-ui-surface border-t border-ui-border flex items-center justify-between shrink-0",

                div {
                    class: "inline-flex items-center gap-1 text-ui-base-muted",
                    Button {
                        variant: ButtonVariant::Icon,
                        disabled: previous_button_style == "disabled",
                        aria_label: "Previous notification".to_string(),
                        onclick: move |_| {
                            let mut model = ui_model.write();
                            model.selected_notification_index = Some(model.selected_notification_index.unwrap_or_default() - 1);
                        },
                        icon_class: "icon-[tabler--chevron-left]".to_string(),
                    }

                    span { class: "text-[11px] font-medium text-ui-base-muted tabular-nums", "{ui_model.read().selected_notification_index.unwrap_or_default() + 1} / {notifications_count()}" }

                    Button {
                        variant: ButtonVariant::Icon,
                        disabled: next_button_style == "disabled",
                        aria_label: "Next notification".to_string(),
                        onclick: move |_| {
                            let mut model = ui_model.write();
                            model.selected_notification_index = Some(model.selected_notification_index.unwrap_or_default() + 1);
                        },
                        icon_class: "icon-[tabler--chevron-right]".to_string(),
                    }
                }

                div {
                    class: "flex items-center gap-1.5 min-w-0",
                    for btn in get_notification_action_buttons(
                        notification,
                        shortcut_visibility_style == "visible") {
                        { btn }
                    }
                }
            }
    }
}

#[component]
fn NotificationDetailsPreview(
    notification: ReadSignal<NotificationWithTask>,
    expand_details: ReadSignal<bool>,
) -> Element {
    match notification().source_item.data {
        ThirdPartyItemData::GithubNotification(github_notification) => {
            match github_notification.item {
                Some(GithubNotificationItem::GithubPullRequest(github_pull_request)) => rsx! {
                    GithubPullRequestPreview { github_pull_request, expand_details }
                },
                Some(GithubNotificationItem::GithubDiscussion(github_discussion)) => rsx! {
                    GithubDiscussionPreview { github_discussion, expand_details }
                },
                _ => rsx! {
                    GithubNotificationDefaultPreview {
                        notification,
                        github_notification: *github_notification
                    }
                },
            }
        }
        ThirdPartyItemData::SlackReaction(slack_reaction) => match slack_reaction.item {
            SlackReactionItem::SlackMessage(slack_message) => rsx! {
                SlackMessagePreview { slack_message, title: notification().title }
            },
            SlackReactionItem::SlackFile(slack_file) => rsx! {
                SlackFilePreview { slack_file, title: notification().title }
            },
        },
        ThirdPartyItemData::SlackThread(slack_thread) => rsx! {
            SlackThreadPreview {
                slack_thread: *slack_thread,
                title: notification().title,
                expand_details
            }
        },
        ThirdPartyItemData::LinearNotification(linear_notification) => rsx! {
            LinearNotificationPreview {
                linear_notification: *linear_notification,
                expand_details
            }
        },
        ThirdPartyItemData::GoogleMailThread(google_mail_thread) => rsx! {
            GoogleMailThreadPreview {
                notification,
                google_mail_thread: *google_mail_thread,
                expand_details,
            }
        },
        ThirdPartyItemData::GoogleCalendarEvent(google_calendar_event) => rsx! {
            GoogleCalendarEventPreview {
                notification,
                google_calendar_event: *google_calendar_event,
                expand_details
            }
        },
        ThirdPartyItemData::GoogleDriveComment(google_drive_comment) => rsx! {
            GoogleDriveCommentPreview {
                notification,
                google_drive_comment: *google_drive_comment,
                expand_details
            }
        },
        ThirdPartyItemData::WebPage(web_page) => rsx! {
            WebPagePreview { notification, web_page: *web_page }
        },
        ThirdPartyItemData::LinearIssue(_)
        | ThirdPartyItemData::TodoistItem(_)
        | ThirdPartyItemData::TickTickItem(_) => rsx! {},
    }
}

fn source_display_name(kind: NotificationSourceKind) -> &'static str {
    // tag: New notification integration
    match kind {
        NotificationSourceKind::Github => "GitHub",
        NotificationSourceKind::Linear => "Linear",
        NotificationSourceKind::GoogleMail => "Gmail",
        NotificationSourceKind::GoogleCalendar => "Google Calendar",
        NotificationSourceKind::GoogleDrive => "Google Drive",
        NotificationSourceKind::Slack => "Slack",
        NotificationSourceKind::Todoist => "Todoist",
        NotificationSourceKind::TickTick => "TickTick",
        NotificationSourceKind::API => "Universal Inbox",
    }
}

fn notification_sub_type(data: &ThirdPartyItemData) -> Option<&'static str> {
    match data {
        ThirdPartyItemData::GithubNotification(n) => match &n.item {
            Some(GithubNotificationItem::GithubPullRequest(_)) => Some("Pull request"),
            Some(GithubNotificationItem::GithubDiscussion(_)) => Some("Discussion"),
            _ => Some("Notification"),
        },
        ThirdPartyItemData::LinearNotification(_) => Some("Notification"),
        ThirdPartyItemData::GoogleMailThread(_) => Some("Email"),
        ThirdPartyItemData::GoogleCalendarEvent(_) => Some("Event"),
        ThirdPartyItemData::GoogleDriveComment(_) => Some("Comment"),
        ThirdPartyItemData::SlackReaction(_) => Some("Reaction"),
        ThirdPartyItemData::SlackThread(_) => Some("Thread"),
        ThirdPartyItemData::TodoistItem(_) | ThirdPartyItemData::TickTickItem(_) => Some("Task"),
        ThirdPartyItemData::WebPage(_) => Some("Web page"),
        ThirdPartyItemData::LinearIssue(_) => None,
    }
}

pub fn get_notification_action_buttons(
    notification: ReadSignal<NotificationWithTask>,
    show_shortcut: bool,
) -> Vec<Element> {
    let context = use_context::<Memo<NotificationListContext>>();

    if !notification().is_built_from_task() {
        let mut buttons = vec![rsx! {
            ActionButton {
                title: "Delete notification",
                shortcut: "d",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service
                        .send(NotificationCommand::DeleteFromNotification(notification()));
                },
                icon_class: "icon-[lucide--trash-2]",
            }
        }];

        if notification().task.is_some() {
            buttons.push(rsx! {
                ActionButton {
                    title: "Complete task",
                    shortcut: "c",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::CompleteTaskFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--check-circle]"
                }
            });
        }

        buttons.push(rsx! {
            ActionButton {
                title: "Unsubscribe from the notification",
                shortcut: "u",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service.send(NotificationCommand::Unsubscribe(notification().id));
                },
                icon_class: "icon-[lucide--bell-off]"
            }
        });

        buttons.push(rsx! {
            ActionButton {
                title: "Snooze notification",
                shortcut: "s",
                show_shortcut,
                onclick: move |_| {
                    context().notification_service.send(NotificationCommand::Snooze(notification().id));
                },
                icon_class: "icon-[lucide--clock]"
            }
        });

        if notification().task.is_none() {
            buttons.push(rsx! {
                ActionButton {
                    title: "Create task",
                    shortcut: "p",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    data_overlay: "#task-planning-modal",
                    icon_class: "icon-[lucide--list-plus]"
                }
            });

            buttons.push(rsx! {
                ActionButton {
                    title: "Create task with defaults",
                    shortcut: "t",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::CreateTaskWithDetaultsFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--zap]"
                }
            });

            buttons.push(rsx! {
                ActionButton {
                    title: "Link to task",
                    shortcut: "l",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    data_overlay: "#task-linking-modal",
                    icon_class: "icon-[lucide--link]"
                }
            });
        }

        buttons
    } else {
        vec![
            rsx! {
                ActionButton {
                    title: "Delete task",
                    shortcut: "d",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::DeleteFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--trash-2]"
                }
            },
            rsx! {
                ActionButton {
                    title: "Complete task",
                    shortcut: "c",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service
                            .send(NotificationCommand::CompleteTaskFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--check-circle]"
                }
            },
            rsx! {
                ActionButton {
                    title: "Snooze notification",
                    shortcut: "s",
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::Snooze(notification().id));
                    },
                    icon_class: "icon-[lucide--clock]"
                }
            },
            rsx! {
                ActionButton {
                    title: "Plan task",
                    shortcut: "p",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    data_overlay: "#task-planning-modal",
                    icon_class: "icon-[lucide--calendar-check]"
                }
            },
            rsx! {
                ActionButton {
                    title: "Create task with defaults",
                    shortcut: "t",
                    disabled_label: (!context().is_task_actions_enabled)
                        .then_some("No task management service connected".to_string()),
                    show_shortcut,
                    onclick: move |_| {
                        context().notification_service.send(NotificationCommand::CreateTaskWithDetaultsFromNotification(notification()));
                    },
                    icon_class: "icon-[lucide--zap]"
                }
            },
        ]
    }
}
