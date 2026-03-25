#![allow(non_snake_case)]

use chrono::{Local, NaiveDate};
use dioxus::prelude::*;

use universal_inbox::{
    task::{DueDate, Task},
    third_party::integrations::todoist::{TodoistItem, TodoistItemDue, TodoistItemPriority},
};

use crate::components::{
    TagDisplay,
    field_grid::{FieldGrid, FieldRow},
    markdown::Markdown,
    preview_card_header::PreviewCardHeader,
    priority_field::{PriorityField, PriorityLevel, priority_color_class},
};

pub fn todoist_priority_level(priority: TodoistItemPriority) -> PriorityLevel {
    match priority {
        TodoistItemPriority::P4 => PriorityLevel::Urgent,
        TodoistItemPriority::P3 => PriorityLevel::High,
        TodoistItemPriority::P2 => PriorityLevel::Normal,
        TodoistItemPriority::P1 => PriorityLevel::Low,
    }
}

#[component]
pub fn TodoistTaskPreview(
    task: ReadSignal<Task>,
    todoist_item: ReadSignal<TodoistItem>,
) -> Element {
    let project_link = task().get_html_project_url();
    let item = todoist_item();

    let is_done = item.checked;
    let title_strike_class = if is_done {
        "line-through opacity-70".to_string()
    } else {
        String::new()
    };

    let header_icon_class = if is_done {
        "icon-[lucide--check-circle] size-4"
    } else {
        "icon-[lucide--circle] size-4"
    };
    let header_icon_color_class = priority_color_class(todoist_priority_level(item.priority));

    // Priority — show number + name in a single muted pill (e.g. "Urgent").
    let priority_label = match item.priority {
        TodoistItemPriority::P1 => "Low",
        TodoistItemPriority::P2 => "Medium",
        TodoistItemPriority::P3 => "High",
        TodoistItemPriority::P4 => "Urgent",
    };

    // Due-date framing — bare row (no pill) with colored calendar icon + date.
    let due_row = item.due.as_ref().map(|due| due_row_props(due));

    // Description / body preview state — collapsed by default with 2-line truncation
    let body_text = task().body.clone();
    let mut expanded = use_signal(|| false);
    let has_body = !body_text.trim().is_empty();

    let project = task().project.clone();
    let project_link_for_sub = project_link.clone();

    let created_date = item.added_at.format("%Y-%m-%d").to_string();
    let created_time = format!("{} UTC", item.added_at.format("%H:%M"));

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! {
                    span { class: "{header_icon_class} {header_icon_color_class}" }
                },
                title: task().title.clone(),
                title_class: title_strike_class,
                subline: rsx! {
                    a {
                        href: "{project_link_for_sub}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "#{project}"
                    }
                }
            }

            div {
                id: "task-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                div {
                    class: "preview-card",

                    FieldGrid {
                        if let Some((color_var, date_label, suffix, is_recurring)) = due_row {
                            FieldRow { label: "Due".to_string(),
                                span {
                                    class: "icon-[lucide--calendar] size-4",
                                    style: "color: {color_var};",
                                }
                                span {
                                    style: "color: {color_var}; font-weight: 600;",
                                    "{date_label}"
                                }
                                if !suffix.is_empty() {
                                    span { class: "text-ui-base-muted opacity-60", "·" }
                                    span { style: "color: var(--ui-base-muted);", "{suffix}" }
                                }
                                if is_recurring {
                                    span {
                                        class: "icon-[lucide--repeat-2] size-3.5 ml-1",
                                        style: "color: var(--ui-base-muted);",
                                        "aria-label": "Recurring",
                                        title: "Recurring"
                                    }
                                }
                            }
                        }

                        PriorityField {
                            label: priority_label.to_string(),
                            level: todoist_priority_level(item.priority),
                        }

                        FieldRow { label: "Created".to_string(),
                            span { class: "font-mono text-xs", "{created_date}" }
                            span { class: "text-ui-base-muted opacity-60", "·" }
                            span { class: "font-mono text-xs", "{created_time}" }
                        }

                        if !item.labels.is_empty() {
                            FieldRow { label: "Labels".to_string(),
                                for label in item.labels.iter().cloned() {
                                    TagDisplay { tag: label.into() }
                                }
                            }
                        }
                    }
                }

                // Description preview — 2-line truncation with inline expand toggle
                if has_body {
                    div {
                        class: "preview-card",

                        div {
                            class: if expanded() { "" } else { "line-clamp-2" },
                            Markdown {
                                class: "prose prose-sm w-full max-w-full",
                                text: body_text.clone()
                            }
                        }

                        button {
                            r#type: "button",
                            class: "text-xs text-primary mt-1 hover:underline",
                            onclick: move |_| expanded.toggle(),
                            if expanded() { "Show less" } else { "Show more" }
                        }
                    }
                }
            }
        }
    }
}

/// Compute due-date color variable, formatted date label, suffix phrase, and recurring flag.
///
/// Colors use CSS custom properties so they pick up the active light/dark theme.
/// - overdue → `--ui-error-text` + `· overdue`
/// - today   → `--ui-warning-text` + `· today`
/// - upcoming → `--ui-base-content` + `· in N days` / `· tomorrow`
fn due_row_props(due: &TodoistItemDue) -> (&'static str, String, String, bool) {
    let today = Local::now().date_naive();
    let due_date = due_date_to_naive(&due.date);
    let label = due_date.format("%b %-d, %Y").to_string();

    let (color_var, suffix) = match due_date.signed_duration_since(today).num_days() {
        d if d < 0 => ("var(--ui-error-text)", "overdue".to_string()),
        0 => ("var(--ui-warning-text)", "today".to_string()),
        1 => ("var(--ui-base-content)", "tomorrow".to_string()),
        d if d <= 7 => ("var(--ui-base-content)", format!("in {d} days")),
        d if d <= 30 => {
            let weeks = (d + 3) / 7;
            (
                "var(--ui-base-content)",
                if weeks == 1 {
                    "in 1 week".to_string()
                } else {
                    format!("in {weeks} weeks")
                },
            )
        }
        d => {
            let months = (d + 15) / 30;
            (
                "var(--ui-base-content)",
                if months <= 1 {
                    "in 1 month".to_string()
                } else {
                    format!("in {months} months")
                },
            )
        }
    };

    (color_var, label, suffix, due.is_recurring)
}

fn due_date_to_naive(date: &DueDate) -> NaiveDate {
    match date {
        DueDate::Date(d) => *d,
        DueDate::DateTime(dt) => dt.date(),
        DueDate::DateTimeWithTz(dt) => dt.with_timezone(&Local).date_naive(),
    }
}
