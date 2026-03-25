#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
pub fn SettingRow(label: Element, description: Option<String>, children: Element) -> Element {
    // Utility composition for the row + label:
    // - Row: flex/space-between, 6px vertical padding, top border on every
    //   row except the first (so adjacent rows in a pod get a divider).
    // - Label: 12px base content, flex with 4px gap. Inline `<code>` tokens
    //   get the monospace pill treatment; nested `<small>` description text
    //   sits below the label in muted color.
    // The `[&_code]:`/`[&_small]:` descendant variants keep all styling
    // anchored in `@theme` tokens (font-mono, bg-ui-surface, text-ui-purple,
    // text-ui-base-muted) so dark mode resolves automatically.
    rsx! {
        div {
            class: "flex items-center justify-between gap-3 py-1.5 \
                    border-t border-ui-border-light first:border-t-0",
            div {
                class: "flex-1 flex items-center gap-1 text-[12px] text-ui-base-content \
                        [&_code]:font-mono [&_code]:text-[10.5px] [&_code]:bg-ui-surface \
                        [&_code]:px-[5px] [&_code]:py-px [&_code]:rounded-ui-xs \
                        [&_code]:text-ui-purple",
                {label}
                if let Some(ref desc) = description {
                    small {
                        class: "block mt-px text-[10.5px] text-ui-base-muted",
                        "{desc}"
                    }
                }
            }
            {children}
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct SegmentedChoiceOption {
    pub value: String,
    pub label: String,
    pub icon_class: Option<String>,
}

impl From<(String, String)> for SegmentedChoiceOption {
    fn from((value, label): (String, String)) -> Self {
        Self {
            value,
            label,
            icon_class: None,
        }
    }
}

#[derive(Props, PartialEq, Clone)]
pub struct SegmentedChoiceProps {
    pub options: Vec<SegmentedChoiceOption>,
    pub selected: String,
    pub on_change: EventHandler<String>,
    #[props(default = false)]
    pub disabled: bool,
    pub aria_label: Option<String>,
}

#[component]
pub fn SegmentedChoice(props: SegmentedChoiceProps) -> Element {
    rsx! {
        div {
            class: "seg",
            role: "tablist",
            aria_label: props.aria_label,
            for option in props.options.iter() {
                {
                    let is_active = option.value == props.selected;
                    let value = option.value.clone();
                    let label = option.label.clone();
                    let icon_class = option.icon_class.clone();
                    let on_change = props.on_change;
                    let disabled = props.disabled;
                    rsx! {
                        button {
                            r#type: "button",
                            role: "tab",
                            class: if is_active { "seg-item active" } else { "seg-item" },
                            aria_selected: "{is_active}",
                            disabled,
                            onclick: move |_| on_change.call(value.clone()),
                            if let Some(icon_class) = icon_class {
                                span { class: "{icon_class} size-4" }
                            }
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}
