#![allow(non_snake_case)]

use chrono::{DateTime, Local, NaiveDate, Utc};
use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::integrations::ticktick::{TickTickItem, TickTickItemPriority, TickTickTaskStatus},
};

use crate::components::{
    TagDisplay,
    field_grid::{FieldGrid, FieldRow},
    markdown::Markdown,
    preview_card_header::PreviewCardHeader,
    priority_field::{PriorityField, PriorityLevel, priority_color_class},
};

pub fn ticktick_priority_level(priority: TickTickItemPriority) -> Option<PriorityLevel> {
    match priority {
        TickTickItemPriority::High => Some(PriorityLevel::High),
        TickTickItemPriority::Medium => Some(PriorityLevel::Normal),
        TickTickItemPriority::Low => Some(PriorityLevel::Low),
        TickTickItemPriority::None => None,
    }
}

#[component]
pub fn TickTickTaskPreview(
    task: ReadSignal<Task>,
    ticktick_item: ReadSignal<TickTickItem>,
) -> Element {
    let project_link = task().get_html_project_url();
    let item = ticktick_item();

    let is_done = item.status == TickTickTaskStatus::Completed;
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
    let header_icon_color_class = ticktick_priority_level(item.priority)
        .map(priority_color_class)
        .unwrap_or("");

    let priority_label = match item.priority {
        TickTickItemPriority::High => "High",
        TickTickItemPriority::Medium => "Medium",
        TickTickItemPriority::Low => "Low",
        TickTickItemPriority::None => "None",
    };

    let due_row = item
        .due_date
        .map(|due| due_row_props(due, item.all_day.unwrap_or(false)));
    let is_recurring = item.is_recurring();

    let body_text = item.content.clone().unwrap_or_default();
    let mut expanded = use_signal(|| false);
    let has_body = !body_text.trim().is_empty();

    let tags = item.tags.clone().unwrap_or_default();
    let project = task().project.clone();
    let project_link_for_sub = project_link.clone();

    let created_date_time = item.created_time.map(|c| {
        (
            c.format("%Y-%m-%d").to_string(),
            format!("{} UTC", c.format("%H:%M")),
        )
    });

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
                        if let Some((color_var, date_label, suffix, time_label)) = due_row {
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
                                if !time_label.is_empty() {
                                    span { class: "text-ui-base-muted opacity-60", "·" }
                                    span { style: "color: var(--ui-base-muted);", "{time_label}" }
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

                        if let Some(level) = ticktick_priority_level(item.priority) {
                            PriorityField {
                                label: priority_label.to_string(),
                                level,
                            }
                        }

                        if let Some((created_date, created_time)) = created_date_time {
                            FieldRow { label: "Created".to_string(),
                                span { class: "font-mono text-xs", "{created_date}" }
                                span { class: "text-ui-base-muted opacity-60", "·" }
                                span { class: "font-mono text-xs", "{created_time}" }
                            }
                        }

                        if !tags.is_empty() {
                            FieldRow { label: "Labels".to_string(),
                                for tag in tags.into_iter() {
                                    TagDisplay { tag: tag.into() }
                                }
                            }
                        }
                    }
                }

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

/// Compute due-date color variable, formatted date label, suffix phrase, and optional time label.
///
/// Mirrors the Todoist `due_row_props` helper so both previews render the Due row identically.
/// Colors use CSS custom properties so they pick up the active light/dark theme.
/// - overdue → `--ui-error-text` + `· overdue`
/// - today   → `--ui-warning-text` + `· today`
/// - upcoming → `--ui-base-content` + `· in N days` / `· tomorrow` / etc.
fn due_row_props(due: DateTime<Utc>, all_day: bool) -> (&'static str, String, String, String) {
    let local = due.with_timezone(&Local);
    let today = Local::now().date_naive();
    let due_date: NaiveDate = local.date_naive();
    let label = due_date.format("%b %-d, %Y").to_string();
    let time_label = if all_day {
        String::new()
    } else {
        local.format("%H:%M").to_string()
    };

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

    (color_var, label, suffix, time_label)
}
