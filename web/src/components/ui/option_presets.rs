//! Domain-specific option builders shared across integration configs.
//!
//! Lives next to the [`UISelect`](super::select::UISelect) primitives because
//! it bridges the design-system component to the app's domain types
//! (`TaskPriority`, `PresetDueDate`). Each helper returns a fresh `Vec` —
//! cheap to call per render, and `UISelectOption` is fully `Clone + PartialEq`.

use dioxus::prelude::*;

use universal_inbox::task::{PresetDueDate, TaskPriority};

use super::{
    select::UISelectOption,
    select_renderers::{PriorityOption, PriorityValue},
};

/// Hex color for a [`TaskPriority`] dot. Aligned with the Universal Inbox
/// semantic palette (`--ui-error`, `--ui-warning`, `--ui-info`, `--ui-primary`).
pub fn task_priority_color(priority: TaskPriority) -> &'static str {
    match priority {
        TaskPriority::P1 => "#E8899A",
        TaskPriority::P2 => "#E8C46A",
        TaskPriority::P3 => "#6BB8D9",
        TaskPriority::P4 => "#388FEF",
    }
}

/// Short qualifier shown right-aligned in the popover row ("Urgent", "High", …).
pub fn task_priority_meta(priority: TaskPriority) -> &'static str {
    match priority {
        TaskPriority::P1 => "Urgent",
        TaskPriority::P2 => "High",
        TaskPriority::P3 => "Medium",
        TaskPriority::P4 => "Normal",
    }
}

/// `(render_value, render_option)` callbacks for a `UISelect<TaskPriority>`.
/// Centralises the trigger/popover rendering used by every integration that
/// surfaces a "default priority" select (Todoist, TickTick, Slack, …).
pub fn priority_select_renderers() -> (
    Callback<UISelectOption<TaskPriority>, Element>,
    Callback<UISelectOption<TaskPriority>, Element>,
) {
    let render_value = use_callback(|opt: UISelectOption<TaskPriority>| {
        rsx! { PriorityValue { color: task_priority_color(opt.value).to_string(), label: opt.label } }
    });
    let render_option = use_callback(|opt: UISelectOption<TaskPriority>| {
        rsx! { PriorityOption {
            color: task_priority_color(opt.value).to_string(),
            label: opt.label,
            meta: opt.meta,
        } }
    });
    (render_value, render_option)
}

/// All four task priorities, with right-aligned meta labels.
pub fn task_priority_options() -> Vec<UISelectOption<TaskPriority>> {
    [
        TaskPriority::P1,
        TaskPriority::P2,
        TaskPriority::P3,
        TaskPriority::P4,
    ]
    .into_iter()
    .map(|p| {
        UISelectOption::new(p, format!("Priority {}", p as u8)).with_meta(task_priority_meta(p))
    })
    .collect()
}

/// Human label for a preset due date ("Today", "Tomorrow", …).
pub fn preset_due_date_label(due: PresetDueDate) -> &'static str {
    match due {
        PresetDueDate::Today => "Today",
        PresetDueDate::Tomorrow => "Tomorrow",
        PresetDueDate::ThisWeekend => "This weekend",
        PresetDueDate::NextWeek => "Next week",
    }
}

/// All four preset due dates with friendly labels.
pub fn preset_due_date_options() -> Vec<UISelectOption<PresetDueDate>> {
    [
        PresetDueDate::Today,
        PresetDueDate::Tomorrow,
        PresetDueDate::ThisWeekend,
        PresetDueDate::NextWeek,
    ]
    .into_iter()
    .map(|d| {
        let label = preset_due_date_label(d.clone());
        UISelectOption::new(d, label)
    })
    .collect()
}
