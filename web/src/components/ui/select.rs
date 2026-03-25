//! Universal Inbox select / combobox primitives.
//!
//! Three components share one popover/keyboard implementation:
//! - [`UISelect`]: plain dropdown, no search.
//! - [`UISearchSelect`]: combobox with built-in filter + optional async loader.
//! - [`UIMultiSelect`]: chips-in-trigger multi-select; popover stays open after pick.
//!
//! All three render against the design tokens in `web/css/universal-inbox.css`
//! (`--ui-primary`, `--ui-base-200`, `--ui-focus-ring`, …) — no hardcoded colors.
//!
//! ## Styling — Tailwind v4 utilities, ARIA + data-attribute compound rules
//!
//! Previously this module relied on ~30 custom `.ss-*` CSS hooks for the
//! trigger, popover, list, row, skeleton, empty, and footer pieces. That layer
//! has been replaced with Tailwind v4 utilities composed in the const class
//! strings + tiny sub-components defined at the top of this file. Three
//! anchors carry the compound state:
//!
//! - **Trigger button** carries `group/select` and the existing
//!   `aria-expanded="true|false"`. Chevron uses `group-aria-expanded/select:rotate-180`.
//! - **Popover row** (each `<li>`) carries `group/row` and three attributes:
//!     - `data-active="true|false"` — keyboard-cursor highlight; drives
//!       `data-[active=true]:bg-ui-primary-subtle data-[active=true]:text-ui-primary-hover`
//!       on the row, and `group-data-[active=true]/row:…` recolors on the
//!       meta / mono / highlight children.
//!     - `aria-selected="true|false"` — currently-chosen value; drives
//!       `aria-selected:font-semibold` and renders the trailing check icon.
//!     - `aria-disabled="true|false"` — drives `aria-disabled:opacity-45 aria-disabled:cursor-not-allowed`.
//! - **Anchor wrapper** carries `data-ui-select-root`, used by external
//!   `.setting-row` rules in `universal-inbox.css` (no `class` hook needed).
//!
//! Two pieces stay in CSS because they cannot be expressed as utilities:
//!
//! 1. `@keyframes ss-pop-in` (popover entrance) and `@keyframes ss-pop-skel`
//!    (loading skeleton shimmer) — registered as
//!    `--animate-ss-pop-in` / `--animate-ss-pop-skel` in the `@theme inline`
//!    block, consumed via `animate-ss-pop-in` / `animate-ss-pop-skel`
//!    utilities.
//! 2. `::-webkit-scrollbar` + `::-webkit-scrollbar-thumb` pseudo-elements on
//!    the popover list — packaged in the `@utility scrollbar-ss { … }`
//!    block, consumed via the `scrollbar-ss` utility on `<ul>`.
//!
//! ## Internal shape
//!
//! The Tailwind class strings live as module-level `const` bindings
//! (`SELECT_TRIGGER_CLASSES`, `POP_ITEM_CLASSES`, …) so each variant references
//! the same canonical string. Three small sub-components
//! ([`UISelectGroupHeader`], [`UISelectFooter`], [`UISelectSkeleton`]) wrap
//! repeated pure-structural pieces.

#![allow(non_snake_case)]
// Many helpers are referenced only through rsx! macro expansions, which
// clippy's dead-code analysis can't always see through. They are exercised
// by the dev smoke harness.
#![allow(dead_code)]

use std::{
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::Element as WebElement;

use crate::components::ui::kbd::Kbd;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct UISelectOption<T: Clone + PartialEq + 'static> {
    pub value: T,
    pub label: String,
    /// Right-aligned muted text (e.g. "Urgent", "12 tasks").
    pub meta: Option<String>,
    /// Optional group header. Consecutive options sharing a `group` get a
    /// small uppercase heading rendered above them.
    pub group: Option<String>,
    pub disabled: bool,
    /// Searched alongside `label` by `UISearchSelect`/`UIMultiSelect`.
    pub search_text: Option<String>,
}

impl<T: Clone + PartialEq + 'static> UISelectOption<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            meta: None,
            group: None,
            disabled: false,
            search_text: None,
        }
    }
    pub fn with_meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = Some(meta.into());
        self
    }
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
    pub fn with_search_text(mut self, search: impl Into<String>) -> Self {
        self.search_text = Some(search.into());
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverPlacement {
    #[default]
    Left,
    Right,
}

// ─── Tailwind class strings ─────────────────────────────────────────────────
//
// Centralized so the three select variants reference one canonical string per
// visual piece. See module-level doc-comment for the data/aria-attribute
// contract that anchors compound rules.

/// Trigger button (`<button>` rendered by `UISelect` / `UISearchSelect` /
/// `UIMultiSelect`). Carries `group/select` so the chevron can react to
/// `aria-expanded` on the same button via `group-aria-expanded/select:`.
pub const SELECT_TRIGGER_CLASSES: &str = "group/select w-full h-9 flex items-center gap-2 pl-3 pr-2.5 \
    border border-ui-border bg-ui-base-200 rounded-ui-md font-ui text-[length:var(--ui-text-base)] \
    text-ui-base-content cursor-pointer text-left select-none \
    transition-[border-color,background,box-shadow] duration-[var(--ui-dur-fast)] ease-[var(--ui-ease)] \
    hover:border-[#cbd5e1] hover:bg-ui-surface \
    aria-expanded:border-ui-primary aria-expanded:bg-ui-surface aria-expanded:shadow-[var(--ui-focus-ring)] \
    focus-visible:outline-none focus-visible:border-ui-primary focus-visible:bg-ui-surface \
    focus-visible:shadow-[var(--ui-focus-ring)] \
    disabled:cursor-not-allowed disabled:opacity-[0.55] disabled:bg-ui-base-200";

/// Chevron icon inside the trigger. Rotates 180° when the popover is open.
pub const SELECT_CHEV_CLASSES: &str = "icon-[lucide--chevron-down] size-3.5 text-ui-base-muted shrink-0 \
    transition-transform duration-[180ms] ease-[var(--ui-ease)] \
    group-aria-expanded/select:rotate-180";

/// Trigger value slot when a value is selected.
pub const SELECT_VALUE_CLASSES: &str = "flex-1 min-w-0 flex items-center gap-2 \
    whitespace-nowrap overflow-hidden text-ellipsis";

/// Trigger value slot when no value is selected — same shape, muted color.
pub const SELECT_VALUE_PLACEHOLDER_CLASSES: &str = "flex-1 min-w-0 flex items-center gap-2 \
    whitespace-nowrap overflow-hidden text-ellipsis text-ui-base-muted";

/// Inline clear button to the right of the value, before the chevron.
pub const SELECT_CLEAR_CLASSES: &str = "inline-flex items-center justify-center w-[18px] h-[18px] \
    rounded-ui-xs border-0 bg-transparent text-ui-base-muted cursor-pointer shrink-0 \
    hover:bg-ui-base-300 hover:text-ui-base-content";

/// Multi-select chip cluster wrapper (replaces the value slot when chips exist).
pub const SELECT_CHIPS_CLASSES: &str = "flex flex-wrap gap-1 flex-1 min-w-0";

/// Individual selected-value chip inside the multi-select trigger.
pub const SELECT_CHIP_CLASSES: &str = "inline-flex items-center gap-1 px-1.5 py-px rounded-ui-pill \
    bg-ui-primary-subtle text-ui-primary-hover text-[length:var(--ui-text-sm)] leading-[1.4] max-w-full";

/// Truncated label inside a chip.
pub const SELECT_CHIP_LABEL_CLASSES: &str =
    "overflow-hidden text-ellipsis whitespace-nowrap max-w-[140px]";

/// Inline ✕ button to remove a chip.
pub const SELECT_CHIP_REMOVE_CLASSES: &str = "inline-flex items-center justify-center w-3 h-3 \
    rounded-ui-xs border-0 bg-transparent text-inherit cursor-pointer shrink-0 opacity-70 \
    hover:opacity-100 hover:bg-[rgba(56,143,239,0.18)]";

/// Absolutely-positioned popover container shared by all three selects.
/// Use `popover_classes(placement)` to pick left vs right placement.
pub const POP_CLASSES_LEFT: &str = "absolute z-50 left-0 right-0 top-[calc(100%+6px)] \
    bg-ui-surface border border-ui-border rounded-ui-lg shadow-ui-lg overflow-hidden \
    flex flex-col origin-top-right animate-ss-pop-in";

pub const POP_CLASSES_RIGHT: &str = "absolute z-50 left-auto right-0 top-[calc(100%+6px)] \
    bg-ui-surface border border-ui-border rounded-ui-lg shadow-ui-lg overflow-hidden \
    flex flex-col origin-top-right animate-ss-pop-in";

pub fn popover_classes(placement: PopoverPlacement) -> &'static str {
    match placement {
        PopoverPlacement::Left => POP_CLASSES_LEFT,
        PopoverPlacement::Right => POP_CLASSES_RIGHT,
    }
}

/// Container around the search input row at the top of the popover.
pub const POP_SEARCH_CLASSES: &str = "p-2 border-b border-ui-border-light bg-ui-surface";

/// Wrapper around the magnifier icon + `<input>` + clear button. Paints the
/// focus halo on `focus-within` so the inner `<input>` doesn't double-up the
/// global input outline.
pub const POP_INPUT_CLASSES: &str = "flex items-center gap-1.5 px-2.5 py-1.5 \
    border border-ui-border bg-ui-base-200 rounded-ui-sm \
    transition-[border-color,background,box-shadow] duration-[var(--ui-dur-fast)] ease-[var(--ui-ease)] \
    focus-within:border-ui-primary focus-within:bg-ui-surface focus-within:shadow-[var(--ui-focus-ring)]";

/// The bare `<input>` inside the search wrapper. We strip the wrapper-level
/// focus outline here so the parent's focus halo is the only visible focus
/// indicator.
pub const POP_INPUT_FIELD_CLASSES: &str = "flex-1 min-w-0 border-0 bg-transparent outline-none \
    text-[length:var(--ui-text-base)] text-ui-base-content placeholder:text-ui-base-muted \
    focus:outline-none focus-visible:outline-none";

/// Inline ✕ button inside the search input wrapper.
pub const POP_INPUT_CLEAR_CLASSES: &str = "bg-transparent border-0 p-0 w-4 h-4 inline-flex \
    items-center justify-center rounded-ui-xs text-ui-base-muted cursor-pointer \
    hover:text-ui-base-content hover:bg-ui-base-300";

/// Scrollable `<ul>` containing the option rows. `scrollbar-ss` is the
/// project-defined utility for the thin slate scrollbar (see
/// `@utility scrollbar-ss` in `universal-inbox.css`).
pub const POP_LIST_CLASSES: &str = "scrollbar-ss list-none m-0 p-1 max-h-60 overflow-y-auto";

/// Group-header `<li>` rendered when a run of options shares the same
/// `group` field.
pub const POP_SECTION_CLASSES: &str = "text-[10px] font-semibold uppercase \
    tracking-[var(--ui-tracking-wider)] text-ui-base-muted pt-2 px-2.5 pb-1";

/// Option row `<li>`. Always carries `group/row` plus `data-active`,
/// `aria-selected`, `aria-disabled`, so the visual state cascades to the
/// label / meta / mono / highlight children.
pub const POP_ITEM_CLASSES: &str = "group/row flex items-center gap-2.5 px-2.5 py-1.5 \
    rounded-ui-sm text-[length:var(--ui-text-base)] text-ui-base-content cursor-pointer \
    transition-colors duration-[80ms] ease-[var(--ui-ease)] \
    hover:bg-ui-surface-hover \
    data-[active=true]:bg-ui-primary-subtle data-[active=true]:text-ui-primary-hover \
    aria-selected:font-semibold \
    aria-disabled:cursor-not-allowed aria-disabled:opacity-45";

/// Truncated label slot inside an option row.
pub const POP_ITEM_LABEL_CLASSES: &str =
    "flex-1 min-w-0 whitespace-nowrap overflow-hidden text-ellipsis";

/// Right-aligned muted meta slot inside an option row. Recolors to
/// `--ui-primary` when the parent row carries `data-active="true"`.
pub const POP_ITEM_META_CLASSES: &str = "text-[length:var(--ui-text-sm)] text-ui-base-muted font-normal \
    group-data-[active=true]/row:text-ui-primary";

/// Monospace subtitle slot inside an option row (used by `EmojiOption`).
pub const POP_ITEM_MONO_CLASSES: &str = "flex-1 min-w-0 font-mono text-[11.5px] text-ui-base-muted \
    font-normal whitespace-nowrap overflow-hidden text-ellipsis \
    group-data-[active=true]/row:text-ui-primary-hover";

/// Trailing check icon shown when a row is currently selected.
pub const POP_ITEM_CHECK_CLASSES: &str =
    "icon-[lucide--check] size-3.5 ml-auto text-ui-primary shrink-0";

/// Loading skeleton container (three shimmer rows for async results).
pub const POP_SKELETON_CLASSES: &str = "flex flex-col gap-1.5 px-1 py-1.5";

/// One shimmer row inside the skeleton. The `animate-ss-pop-skel` utility
/// sweeps the linear-gradient backdrop left-to-right indefinitely.
pub const POP_SKELETON_ROW_CLASSES: &str = "h-7 rounded-ui-sm \
    bg-[linear-gradient(90deg,var(--ui-base-200)_0%,var(--ui-surface-hover)_50%,var(--ui-base-200)_100%)] \
    bg-[length:200%_100%] animate-ss-pop-skel";

/// "No options" / "No matches" empty-state container. Includes a
/// descendant rule that styles the embedded `<mark>` (the query echo
/// inside "No matches for X").
pub const POP_EMPTY_CLASSES: &str = "px-3 py-6 text-center text-ui-base-muted text-[11.5px] \
    flex flex-col items-center gap-1.5 \
    [&_mark]:bg-transparent [&_mark]:text-ui-base-content [&_mark]:font-semibold";

/// Empty-state icon slot.
pub const POP_EMPTY_ICON_CLASSES: &str = "opacity-50";

/// Optional smaller-text hint inside the empty state.
pub const POP_EMPTY_HINT_CLASSES: &str = "text-[10.5px]";

/// Footer keyboard-hint bar at the bottom of the popover.
pub const POP_FOOT_CLASSES: &str = "flex items-center gap-3.5 px-2.5 py-1.5 \
    border-t border-ui-border-light bg-ui-base-200 \
    text-[length:var(--ui-text-sm)] text-ui-base-muted";

/// Footer spacer that pushes the trailing hint to the right.
pub const POP_FOOT_SPACER_CLASSES: &str = "flex-1";

/// Single `Kbd + label` cluster inside the footer.
pub const POP_HINT_CLASSES: &str = "inline-flex items-center gap-1.5";

/// Substring-highlight `<mark>` rendered by [`ss_highlight`]. Inverts to a
/// lighter background when the ancestor row carries `data-active="true"`.
pub const MARK_HIGHLIGHT_CLASSES: &str = "bg-[rgba(56,143,239,0.18)] text-ui-base-content \
    rounded-[2px] px-px group-data-[active=true]/row:bg-white/60";

// ─── Helpers ────────────────────────────────────────────────────────────────

static SELECT_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn next_select_id() -> String {
    let n = SELECT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ui-select-{n}")
}

/// Highlight the first case-insensitive occurrence of `query` inside `text`.
/// Wraps the match in a styled `<mark>` that inverts to a lighter background
/// when its ancestor row carries `data-active="true"` (the keyboard-cursor row).
pub fn ss_highlight(text: &str, query: &str) -> Element {
    if query.is_empty() {
        return rsx! { "{text}" };
    }
    let lower_text = text.to_lowercase();
    let lower_query = query.to_lowercase();
    let Some(idx) = lower_text.find(&lower_query) else {
        return rsx! { "{text}" };
    };
    let before = &text[..idx];
    let matched = &text[idx..idx + query.len()];
    let after = &text[idx + query.len()..];
    rsx! {
        "{before}"
        mark { class: MARK_HIGHLIGHT_CLASSES, "{matched}" }
        "{after}"
    }
}

/// Scroll the popover row identified by `option_id` into view, "nearest" block
/// alignment so we don't aggressively jump to top/bottom.
fn scroll_active_into_view(option_id: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(el) = document.get_element_by_id(option_id) else {
        return;
    };
    let opts = web_sys::ScrollIntoViewOptions::new();
    opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
    el.scroll_into_view_with_scroll_into_view_options(&opts);
}

/// Focus an element by id (used to return focus to the trigger on close).
fn focus_element(id: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(el) = document.get_element_by_id(id) else {
        return;
    };
    if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
        let _ = html_el.focus();
    }
}

/// RAII guard that owns the document-level mousedown/keydown closures and
/// removes them from the DOM on Drop. Stored in a `use_hook` slot so it lives
/// for the component's lifetime and is dropped on unmount — keeping the
/// listeners properly bounded. Without this, the closures would leak and stale
/// captures could fire on navigated-away components, panicking on the
/// deallocated signals.
struct OutsideCloseGuard {
    document: web_sys::Document,
    mousedown: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    keydown: Option<Closure<dyn FnMut(web_sys::KeyboardEvent)>>,
}

impl Drop for OutsideCloseGuard {
    fn drop(&mut self) {
        if let Some(c) = self.mousedown.take() {
            let _ = self
                .document
                .remove_event_listener_with_callback("mousedown", c.as_ref().unchecked_ref());
        }
        if let Some(c) = self.keydown.take() {
            let _ = self
                .document
                .remove_event_listener_with_callback("keydown", c.as_ref().unchecked_ref());
        }
    }
}

/// Attach document-level `mousedown` and `keydown` listeners that close the
/// popover on outside click or Escape.
///
/// Signals are read via `.peek()` to avoid creating reactive subscriptions in
/// closures that run in the raw WASM event-loop (outside any Dioxus scope) —
/// a tracked read there would panic.
///
/// Listeners are removed on component unmount via `OutsideCloseGuard::drop`.
pub fn use_outside_close(
    wrapper_el: Signal<Option<WebElement>>,
    is_open: Signal<bool>,
    on_close: impl FnMut() + Clone + 'static,
) {
    use_hook(move || -> Option<Rc<OutsideCloseGuard>> {
        let window = web_sys::window()?;
        let document = window.document()?;

        let mut on_close_md = on_close.clone();
        let mousedown = Closure::wrap(Box::new(move |evt: web_sys::MouseEvent| {
            if !*is_open.peek() {
                return;
            }
            let Some(target) = evt.target() else {
                return;
            };
            let wrapper_ref = wrapper_el.peek();
            let Some(wrapper) = wrapper_ref.as_ref() else {
                return;
            };
            let Some(node) = target.dyn_ref::<web_sys::Node>() else {
                return;
            };
            if !wrapper.contains(Some(node)) {
                on_close_md();
            }
        }) as Box<dyn FnMut(web_sys::MouseEvent)>);
        document
            .add_event_listener_with_callback("mousedown", mousedown.as_ref().unchecked_ref())
            .ok()?;

        let mut on_close_esc = on_close;
        let keydown = Closure::wrap(Box::new(move |evt: web_sys::KeyboardEvent| {
            if *is_open.peek() && evt.key() == "Escape" {
                on_close_esc();
                evt.stop_propagation();
            }
        }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
        document
            .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
            .ok()?;

        Some(Rc::new(OutsideCloseGuard {
            document,
            mousedown: Some(mousedown),
            keydown: Some(keydown),
        }))
    });
}

/// Filter options locally by `query` against `label` + `search_text`.
fn filter_local<T: Clone + PartialEq + 'static>(
    options: &[UISelectOption<T>],
    query: &str,
) -> Vec<UISelectOption<T>> {
    if query.is_empty() {
        return options.to_vec();
    }
    let q = query.to_lowercase();
    options
        .iter()
        .filter(|o| {
            o.label.to_lowercase().contains(&q)
                || o.search_text
                    .as_deref()
                    .map(|s| s.to_lowercase().contains(&q))
                    .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// Render trigger value text. Default: option label, falling back to placeholder.
fn render_value_default<T: Clone + PartialEq + 'static>(
    selected: Option<&UISelectOption<T>>,
    placeholder: &str,
) -> Element {
    match selected {
        Some(o) => rsx! { "{o.label}" },
        None => rsx! { "{placeholder}" },
    }
}

// ─── Render-fn slot type ────────────────────────────────────────────────────
//
// Dioxus props need `Clone + PartialEq`. Closures don't impl PartialEq, so we
// can't put them in props directly. `Callback<I, O>` from Dioxus solves that
// for `Fn(I) -> O` shapes — it's `Copy` and PartialEq-compatible.

type RenderOption<T> = Callback<UISelectOption<T>, Element>;
type RenderOptionWithQuery<T> = Callback<(UISelectOption<T>, String), Element>;

// ─── UISelect ───────────────────────────────────────────────────────────────

#[derive(Props, Clone)]
pub struct UISelectProps<T: Clone + PartialEq + 'static> {
    pub value: Signal<Option<T>>,
    pub options: Vec<UISelectOption<T>>,
    pub on_change: EventHandler<Option<T>>,

    #[props(default = "Select…".to_string())]
    pub placeholder: String,
    #[props(default = false)]
    pub allow_clear: bool,
    #[props(default = false)]
    pub disabled: bool,
    #[props(default)]
    pub popover_placement: PopoverPlacement,
    #[props(default)]
    pub width: Option<String>,
    /// Optional aria-label / DOM name. Useful for tests + a11y when no visible
    /// label is wired via a wrapping `<label>`.
    #[props(default)]
    pub name: Option<String>,
    #[props(default)]
    pub render_value: Option<RenderOption<T>>,
    #[props(default)]
    pub render_option: Option<RenderOption<T>>,

    #[props(default)]
    pub phantom: PhantomData<T>,
}

impl<T: Clone + PartialEq + 'static> PartialEq for UISelectProps<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
            && self.options == other.options
            && self.on_change == other.on_change
            && self.placeholder == other.placeholder
            && self.allow_clear == other.allow_clear
            && self.disabled == other.disabled
            && self.popover_placement == other.popover_placement
            && self.width == other.width
            && self.name == other.name
            && self.render_value == other.render_value
            && self.render_option == other.render_option
    }
}

#[component]
pub fn UISelect<T: Clone + PartialEq + 'static>(props: UISelectProps<T>) -> Element {
    let id = use_hook(next_select_id);
    let trigger_id = format!("{id}-trigger");
    let popover_id = format!("{id}-pop");

    let mut is_open = use_signal(|| false);
    let mut active = use_signal(|| 0usize);
    let mut wrapper_el: Signal<Option<WebElement>> = use_signal(|| None);

    let options = props.options.clone();
    let placeholder = props.placeholder.clone();
    let disabled = props.disabled;
    let allow_clear = props.allow_clear;
    let render_value = props.render_value;
    let render_option = props.render_option;
    let popover_placement = props.popover_placement;
    let width = props.width.clone();
    let name = props.name.clone();
    let on_change = props.on_change;
    let mut value = props.value;

    // Initialize active row to current selection when opening.
    use_effect({
        let options = options.clone();
        let value = value;
        move || {
            if is_open()
                && let Some(v) = value()
                && let Some(idx) = options.iter().position(|o| o.value == v)
            {
                active.set(idx);
            }
        }
    });

    // Outside click + Esc to close.
    let close = {
        let trigger_id = trigger_id.clone();
        move || {
            if *is_open.peek() {
                is_open.set(false);
                focus_element(&trigger_id);
            }
        }
    };
    use_outside_close(wrapper_el, is_open, close.clone());

    let selected = options
        .iter()
        .find(|o| Some(&o.value) == value().as_ref())
        .cloned();
    let pop_class = popover_classes(popover_placement);
    let style = width.map(|w| format!("width:{w};")).unwrap_or_default();

    let move_active = {
        let len = options.len();
        let popover_id = popover_id.clone();
        move |delta: isize| {
            let new = if delta > 0 {
                (active() + 1).min(len.saturating_sub(1))
            } else {
                active().saturating_sub(1)
            };
            active.set(new);
            scroll_active_into_view(&format!("{popover_id}-opt-{new}"));
        }
    };

    let pick = {
        let mut close = close.clone();
        move |opt: UISelectOption<T>| {
            if opt.disabled {
                return;
            }
            value.set(Some(opt.value.clone()));
            on_change.call(Some(opt.value));
            close();
        }
    };

    rsx! {
        div {
            class: "relative w-full block",
            "data-ui-select-root": true,
            style: "{style}",
            onmounted: move |evt| {
                wrapper_el.set(Some(evt.as_web_event()));
            },

            button {
                r#type: "button",
                id: "{trigger_id}",
                "data-ui-select-trigger": true,
                class: SELECT_TRIGGER_CLASSES,
                disabled,
                "aria-haspopup": "listbox",
                "aria-expanded": is_open(),
                "aria-controls": "{popover_id}",
                "aria-label": name.clone().unwrap_or_default(),
                onclick: move |_| {
                    if !disabled {
                        is_open.set(!is_open());
                    }
                },
                onkeydown: {
                    let options_kd = options.clone();
                    let mut move_active_kd = move_active.clone();
                    let mut pick_kd = pick.clone();
                    move |evt: KeyboardEvent| {
                        let key = evt.key();
                        if !is_open() {
                            if key == Key::ArrowDown || key == Key::Enter || key == Key::Character(" ".into()) {
                                evt.prevent_default();
                                is_open.set(true);
                            }
                        } else {
                            match key {
                                Key::ArrowDown => { evt.prevent_default(); move_active_kd(1); }
                                Key::ArrowUp => { evt.prevent_default(); move_active_kd(-1); }
                                Key::Enter => {
                                    evt.prevent_default();
                                    if let Some(opt) = options_kd.get(active()).cloned() {
                                        pick_kd(opt);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                },

                span {
                    "data-ui-select-value": true,
                    class: if selected.is_some() { SELECT_VALUE_CLASSES } else { SELECT_VALUE_PLACEHOLDER_CLASSES },
                    if let Some(opt) = &selected {
                        if let Some(rv) = render_value {
                            { rv(opt.clone()) }
                        } else {
                            { render_value_default(Some(opt), &placeholder) }
                        }
                    } else {
                        "{placeholder}"
                    }
                }

                if allow_clear && selected.is_some() && !disabled {
                    span {
                        role: "button",
                        class: SELECT_CLEAR_CLASSES,
                        "aria-label": "Clear",
                        onclick: move |evt: Event<MouseData>| {
                            evt.stop_propagation();
                            value.set(None);
                            on_change.call(None);
                        },
                        span { class: "icon-[lucide--x] size-3", "aria-hidden": "true" }
                    }
                }
                span {
                    "data-ui-select-chev": true,
                    class: SELECT_CHEV_CLASSES,
                    "aria-hidden": "true",
                }
            }

            if is_open() {
                div {
                    id: "{popover_id}",
                    "data-ui-select-popover": true,
                    class: pop_class,
                    role: "listbox",
                    tabindex: -1,
                    ul { class: POP_LIST_CLASSES,
                        {render_options_list(
                            &options,
                            value(),
                            active(),
                            &popover_id,
                            "",
                            render_option,
                            &mut active,
                            pick.clone(),
                        )}
                    }
                }
            }
        }
    }
}

// Helper: render the option list (shared by UISelect / UISearchSelect / UIMultiSelect).
// Splits into group sections when consecutive options share a `group`.
#[allow(clippy::too_many_arguments)]
fn render_options_list<T: Clone + PartialEq + 'static>(
    options: &[UISelectOption<T>],
    value: Option<T>,
    active: usize,
    popover_id: &str,
    query: &str,
    render_option: Option<RenderOption<T>>,
    active_signal: &mut Signal<usize>,
    pick: impl FnMut(UISelectOption<T>) + 'static + Clone,
) -> Element {
    let mut active_signal = *active_signal;
    let value_clone = value.clone();
    let mut current_group: Option<String> = None;
    let mut nodes: Vec<Element> = Vec::with_capacity(options.len());

    for (idx, opt) in options.iter().enumerate() {
        let group_changed = opt.group.as_deref() != current_group.as_deref();
        if group_changed {
            if let Some(g) = &opt.group {
                let g = g.clone();
                nodes.push(rsx! {
                    li {
                        key: "{popover_id}-grp-{idx}",
                        class: POP_SECTION_CLASSES,
                        role: "presentation",
                        "{g}"
                    }
                });
            }
            current_group = opt.group.clone();
        }

        let opt_id = format!("{popover_id}-opt-{idx}");
        let is_selected = value_clone.as_ref() == Some(&opt.value);
        let is_active = active == idx;
        let is_disabled = opt.disabled;
        let opt_for_pick = opt.clone();
        let opt_for_render = opt.clone();
        let mut pick = pick.clone();
        let query_owned = query.to_string();

        nodes.push(rsx! {
            li {
                key: "{opt_id}",
                id: "{opt_id}",
                role: "option",
                "aria-selected": is_selected,
                "aria-disabled": is_disabled,
                "data-active": is_active,
                class: POP_ITEM_CLASSES,
                onmouseenter: move |_| {
                    if !is_disabled {
                        active_signal.set(idx);
                    }
                },
                onclick: move |_| {
                    if !is_disabled {
                        pick(opt_for_pick.clone());
                    }
                },

                if let Some(rop) = render_option {
                    { rop(opt_for_render.clone()) }
                } else {
                    span { class: POP_ITEM_LABEL_CLASSES,
                        { ss_highlight(&opt_for_render.label, &query_owned) }
                    }
                    if let Some(meta) = &opt_for_render.meta {
                        span { class: POP_ITEM_META_CLASSES, "{meta}" }
                    }
                }

                if is_selected {
                    span {
                        class: POP_ITEM_CHECK_CLASSES,
                        "aria-hidden": "true",
                    }
                }
            }
        });
    }

    rsx! {
        for node in nodes.into_iter() { { node } }
    }
}

// ─── UISearchSelect ─────────────────────────────────────────────────────────

#[derive(Props, Clone)]
pub struct UISearchSelectProps<T: Clone + PartialEq + 'static> {
    pub value: Signal<Option<T>>,
    /// Options to render. When `on_query` is `None`, filtering is local; when
    /// `on_query` is `Some`, the consumer is expected to update `options` in
    /// response to the query (server-side filtering).
    pub options: Vec<UISelectOption<T>>,
    pub on_change: EventHandler<Option<T>>,

    /// When set, the consumer manages filtering. Each keystroke fires this
    /// handler with the current query; pass results back via `options`.
    #[props(default)]
    pub on_query: Option<EventHandler<String>>,
    /// Render a 3-row skeleton instead of the list while async results load.
    #[props(default = false)]
    pub loading: bool,

    #[props(default = "Select…".to_string())]
    pub placeholder: String,
    #[props(default = "Search…".to_string())]
    pub search_placeholder: String,
    #[props(default = true)]
    pub allow_clear: bool,
    #[props(default = false)]
    pub disabled: bool,
    #[props(default)]
    pub popover_placement: PopoverPlacement,
    #[props(default)]
    pub width: Option<String>,
    #[props(default)]
    pub name: Option<String>,
    #[props(default)]
    pub empty_hint: Option<String>,
    #[props(default)]
    pub render_value: Option<RenderOption<T>>,
    #[props(default)]
    pub render_option: Option<RenderOptionWithQuery<T>>,

    #[props(default)]
    pub phantom: PhantomData<T>,
}

impl<T: Clone + PartialEq + 'static> PartialEq for UISearchSelectProps<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
            && self.options == other.options
            && self.on_change == other.on_change
            && self.on_query == other.on_query
            && self.loading == other.loading
            && self.placeholder == other.placeholder
            && self.search_placeholder == other.search_placeholder
            && self.allow_clear == other.allow_clear
            && self.disabled == other.disabled
            && self.popover_placement == other.popover_placement
            && self.width == other.width
            && self.name == other.name
            && self.empty_hint == other.empty_hint
            && self.render_value == other.render_value
            && self.render_option == other.render_option
    }
}

#[component]
pub fn UISearchSelect<T: Clone + PartialEq + 'static>(props: UISearchSelectProps<T>) -> Element {
    let id = use_hook(next_select_id);
    let trigger_id = format!("{id}-trigger");
    let popover_id = format!("{id}-pop");
    let input_id = format!("{id}-input");

    let mut is_open = use_signal(|| false);
    let mut active = use_signal(|| 0usize);
    let mut query = use_signal(String::new);
    let mut wrapper_el: Signal<Option<WebElement>> = use_signal(|| None);

    let options_input = props.options.clone();
    let on_query = props.on_query;
    let server_filtering = on_query.is_some();
    let loading = props.loading;
    let placeholder = props.placeholder.clone();
    let search_placeholder = props.search_placeholder.clone();
    let empty_hint = props.empty_hint.clone();
    let disabled = props.disabled;
    let allow_clear = props.allow_clear;
    let render_value = props.render_value;
    let render_option = props.render_option;
    let popover_placement = props.popover_placement;
    let width = props.width.clone();
    let name = props.name.clone();
    let on_change = props.on_change;
    let mut value = props.value;

    // Filter locally if no on_query, else trust upstream.
    let filtered: Vec<UISelectOption<T>> = if server_filtering {
        options_input.clone()
    } else {
        filter_local(&options_input, &query())
    };

    // Reset active row when query/openness changes.
    use_effect(move || {
        let _ = (query(), is_open());
        active.set(0);
    });

    // Focus the search input when opening.
    use_effect({
        let input_id = input_id.clone();
        move || {
            if is_open() {
                focus_element(&input_id);
            }
        }
    });

    let close = {
        let trigger_id = trigger_id.clone();
        move || {
            if *is_open.peek() {
                is_open.set(false);
                query.set(String::new());
                focus_element(&trigger_id);
            }
        }
    };
    use_outside_close(wrapper_el, is_open, close.clone());

    let selected = options_input
        .iter()
        .find(|o| Some(&o.value) == value().as_ref())
        .cloned();
    let pop_class = popover_classes(popover_placement);
    let style = width.map(|w| format!("width:{w};")).unwrap_or_default();

    let move_active = {
        let len = filtered.len();
        let popover_id = popover_id.clone();
        move |delta: isize| {
            let new = if delta > 0 {
                (active() + 1).min(len.saturating_sub(1))
            } else {
                active().saturating_sub(1)
            };
            active.set(new);
            scroll_active_into_view(&format!("{popover_id}-opt-{new}"));
        }
    };

    let pick = {
        let mut close = close.clone();
        move |opt: UISelectOption<T>| {
            if opt.disabled {
                return;
            }
            value.set(Some(opt.value.clone()));
            on_change.call(Some(opt.value));
            close();
        }
    };

    rsx! {
        div {
            class: "relative w-full block",
            "data-ui-select-root": true,
            style: "{style}",
            onmounted: move |evt| {
                wrapper_el.set(Some(evt.as_web_event()));
            },

            button {
                r#type: "button",
                id: "{trigger_id}",
                "data-ui-select-trigger": true,
                class: SELECT_TRIGGER_CLASSES,
                disabled,
                "aria-haspopup": "listbox",
                "aria-expanded": is_open(),
                "aria-controls": "{popover_id}",
                "aria-label": name.clone().unwrap_or_default(),
                onclick: move |_| {
                    if !disabled {
                        is_open.set(!is_open());
                    }
                },

                span {
                    "data-ui-select-value": true,
                    class: if selected.is_some() { SELECT_VALUE_CLASSES } else { SELECT_VALUE_PLACEHOLDER_CLASSES },
                    if let Some(opt) = &selected {
                        if let Some(rv) = render_value {
                            { rv(opt.clone()) }
                        } else {
                            { render_value_default(Some(opt), &placeholder) }
                        }
                    } else {
                        "{placeholder}"
                    }
                }

                if allow_clear && selected.is_some() && !disabled {
                    span {
                        role: "button",
                        class: SELECT_CLEAR_CLASSES,
                        "aria-label": "Clear",
                        onclick: move |evt: Event<MouseData>| {
                            evt.stop_propagation();
                            value.set(None);
                            on_change.call(None);
                        },
                        span { class: "icon-[lucide--x] size-3", "aria-hidden": "true" }
                    }
                }
                span {
                    "data-ui-select-chev": true,
                    class: SELECT_CHEV_CLASSES,
                    "aria-hidden": "true",
                }
            }

            if is_open() {
                div {
                    id: "{popover_id}",
                    "data-ui-select-popover": true,
                    class: pop_class,
                    div { class: POP_SEARCH_CLASSES,
                        div { class: POP_INPUT_CLASSES,
                            span {
                                class: "icon-[lucide--search] size-3.5 text-ui-base-muted shrink-0",
                                "aria-hidden": "true",
                            }
                            input {
                                id: "{input_id}",
                                // The wrapper paints the focus halo via `focus-within:`;
                                // opt the input itself out of the global `input:focus-visible`
                                // outline so the wrapper halo is the only ring.
                                class: POP_INPUT_FIELD_CLASSES,
                                r#type: "text",
                                value: "{query}",
                                placeholder: "{search_placeholder}",
                                "aria-label": "{search_placeholder}",
                                "aria-controls": "{popover_id}",
                                "aria-autocomplete": "list",
                                oninput: move |evt| {
                                    let q = evt.value();
                                    query.set(q.clone());
                                    if let Some(handler) = on_query {
                                        handler.call(q);
                                    }
                                },
                                onkeydown: {
                                    let filtered_kd = filtered.clone();
                                    let mut move_active_kd = move_active.clone();
                                    let mut pick_kd = pick.clone();
                                    let mut close_kd = close.clone();
                                    move |evt: KeyboardEvent| {
                                        match evt.key() {
                                            Key::ArrowDown => { evt.prevent_default(); move_active_kd(1); }
                                            Key::ArrowUp => { evt.prevent_default(); move_active_kd(-1); }
                                            Key::Enter => {
                                                evt.prevent_default();
                                                if let Some(opt) = filtered_kd.get(active()).cloned() {
                                                    pick_kd(opt);
                                                }
                                            }
                                            Key::Escape => {
                                                evt.prevent_default();
                                                close_kd();
                                            }
                                            _ => {}
                                        }
                                    }
                                },
                            }
                            if !query().is_empty() {
                                button {
                                    r#type: "button",
                                    class: POP_INPUT_CLEAR_CLASSES,
                                    "aria-label": "Clear search",
                                    onclick: move |_| {
                                        query.set(String::new());
                                        if let Some(handler) = on_query {
                                            handler.call(String::new());
                                        }
                                    },
                                    span { class: "icon-[lucide--x] size-3", "aria-hidden": "true" }
                                }
                            }
                        }
                    }

                    if loading {
                        div { class: POP_SKELETON_CLASSES,
                            div { class: POP_SKELETON_ROW_CLASSES }
                            div { class: POP_SKELETON_ROW_CLASSES }
                            div { class: POP_SKELETON_ROW_CLASSES }
                        }
                    } else if filtered.is_empty() {
                        div { class: POP_EMPTY_CLASSES,
                            span {
                                class: "icon-[lucide--search-x] size-5 opacity-50",
                                "aria-hidden": "true",
                            }
                            div {
                                "No matches for "
                                mark { "\"{query}\"" }
                            }
                            if let Some(hint) = &empty_hint {
                                div { class: POP_EMPTY_HINT_CLASSES, "{hint}" }
                            }
                        }
                    } else {
                        ul { class: POP_LIST_CLASSES,
                            {render_options_list_search(
                                &filtered,
                                value(),
                                active(),
                                &popover_id,
                                &query(),
                                render_option,
                                &mut active,
                                pick.clone(),
                            )}
                        }
                    }

                    div { class: POP_FOOT_CLASSES,
                        span { class: POP_HINT_CLASSES,
                            Kbd { label: "↑" }
                            Kbd { label: "↓" }
                            " Navigate"
                        }
                        span { class: POP_HINT_CLASSES,
                            Kbd { label: "↵" }
                            " Select"
                        }
                        span { class: POP_FOOT_SPACER_CLASSES }
                        span { class: POP_HINT_CLASSES,
                            Kbd { label: "esc" }
                            " Close"
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_options_list_search<T: Clone + PartialEq + 'static>(
    options: &[UISelectOption<T>],
    value: Option<T>,
    active: usize,
    popover_id: &str,
    query: &str,
    render_option: Option<RenderOptionWithQuery<T>>,
    active_signal: &mut Signal<usize>,
    pick: impl FnMut(UISelectOption<T>) + 'static + Clone,
) -> Element {
    let mut active_signal = *active_signal;
    let value_clone = value.clone();
    let mut current_group: Option<String> = None;
    let mut nodes: Vec<Element> = Vec::with_capacity(options.len());

    for (idx, opt) in options.iter().enumerate() {
        let group_changed = opt.group.as_deref() != current_group.as_deref();
        if group_changed {
            if let Some(g) = &opt.group {
                let g = g.clone();
                nodes.push(rsx! {
                    li {
                        key: "{popover_id}-grp-{idx}",
                        class: POP_SECTION_CLASSES,
                        role: "presentation",
                        "{g}"
                    }
                });
            }
            current_group = opt.group.clone();
        }

        let opt_id = format!("{popover_id}-opt-{idx}");
        let is_selected = value_clone.as_ref() == Some(&opt.value);
        let is_active = active == idx;
        let is_disabled = opt.disabled;
        let opt_for_pick = opt.clone();
        let opt_for_render = opt.clone();
        let mut pick = pick.clone();
        let query_owned = query.to_string();

        nodes.push(rsx! {
            li {
                key: "{opt_id}",
                id: "{opt_id}",
                role: "option",
                "aria-selected": is_selected,
                "aria-disabled": is_disabled,
                "data-active": is_active,
                class: POP_ITEM_CLASSES,
                onmouseenter: move |_| {
                    if !is_disabled {
                        active_signal.set(idx);
                    }
                },
                onclick: move |_| {
                    if !is_disabled {
                        pick(opt_for_pick.clone());
                    }
                },

                if let Some(rop) = render_option {
                    { rop((opt_for_render.clone(), query_owned.clone())) }
                } else {
                    span { class: POP_ITEM_LABEL_CLASSES,
                        { ss_highlight(&opt_for_render.label, &query_owned) }
                    }
                    if let Some(meta) = &opt_for_render.meta {
                        span { class: POP_ITEM_META_CLASSES, "{meta}" }
                    }
                }

                if is_selected {
                    span {
                        class: POP_ITEM_CHECK_CLASSES,
                        "aria-hidden": "true",
                    }
                }
            }
        });
    }

    rsx! {
        for node in nodes.into_iter() { { node } }
    }
}

// ─── UIMultiSelect ──────────────────────────────────────────────────────────

#[derive(Props, Clone)]
pub struct UIMultiSelectProps<T: Clone + PartialEq + 'static> {
    pub value: Signal<Vec<T>>,
    pub options: Vec<UISelectOption<T>>,
    pub on_change: EventHandler<Vec<T>>,

    #[props(default)]
    pub on_query: Option<EventHandler<String>>,
    #[props(default = false)]
    pub loading: bool,
    #[props(default = false)]
    pub searchable: bool,

    #[props(default = "Select…".to_string())]
    pub placeholder: String,
    #[props(default = "Search…".to_string())]
    pub search_placeholder: String,
    #[props(default = false)]
    pub disabled: bool,
    #[props(default)]
    pub popover_placement: PopoverPlacement,
    #[props(default)]
    pub width: Option<String>,
    #[props(default)]
    pub name: Option<String>,
    #[props(default)]
    pub render_chip: Option<RenderOption<T>>,
    #[props(default)]
    pub render_option: Option<RenderOptionWithQuery<T>>,

    #[props(default)]
    pub phantom: PhantomData<T>,
}

impl<T: Clone + PartialEq + 'static> PartialEq for UIMultiSelectProps<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
            && self.options == other.options
            && self.on_change == other.on_change
            && self.on_query == other.on_query
            && self.loading == other.loading
            && self.searchable == other.searchable
            && self.placeholder == other.placeholder
            && self.search_placeholder == other.search_placeholder
            && self.disabled == other.disabled
            && self.popover_placement == other.popover_placement
            && self.width == other.width
            && self.name == other.name
            && self.render_chip == other.render_chip
            && self.render_option == other.render_option
    }
}

#[component]
pub fn UIMultiSelect<T: Clone + PartialEq + 'static>(props: UIMultiSelectProps<T>) -> Element {
    let id = use_hook(next_select_id);
    let trigger_id = format!("{id}-trigger");
    let popover_id = format!("{id}-pop");
    let input_id = format!("{id}-input");

    let mut is_open = use_signal(|| false);
    let mut active = use_signal(|| 0usize);
    let mut query = use_signal(String::new);
    let mut wrapper_el: Signal<Option<WebElement>> = use_signal(|| None);

    let options_input = props.options.clone();
    let on_query = props.on_query;
    let server_filtering = on_query.is_some();
    let loading = props.loading;
    let searchable = props.searchable || server_filtering;
    let placeholder = props.placeholder.clone();
    let search_placeholder = props.search_placeholder.clone();
    let disabled = props.disabled;
    let render_chip = props.render_chip;
    let render_option = props.render_option;
    let popover_placement = props.popover_placement;
    let width = props.width.clone();
    let name = props.name.clone();
    let on_change = props.on_change;
    let mut value = props.value;

    let filtered: Vec<UISelectOption<T>> = if server_filtering {
        options_input.clone()
    } else if searchable {
        filter_local(&options_input, &query())
    } else {
        options_input.clone()
    };

    use_effect(move || {
        let _ = (query(), is_open());
        active.set(0);
    });
    use_effect({
        let input_id = input_id.clone();
        move || {
            if is_open() && searchable {
                focus_element(&input_id);
            }
        }
    });

    let close = {
        let trigger_id = trigger_id.clone();
        move || {
            if *is_open.peek() {
                is_open.set(false);
                query.set(String::new());
                focus_element(&trigger_id);
            }
        }
    };
    use_outside_close(wrapper_el, is_open, close.clone());

    let selected_values = value();
    let selected_options: Vec<UISelectOption<T>> = options_input
        .iter()
        .filter(|o| selected_values.contains(&o.value))
        .cloned()
        .collect();

    let pop_class = popover_classes(popover_placement);
    let style = width.map(|w| format!("width:{w};")).unwrap_or_default();

    let move_active = {
        let len = filtered.len();
        let popover_id = popover_id.clone();
        move |delta: isize| {
            let new = if delta > 0 {
                (active() + 1).min(len.saturating_sub(1))
            } else {
                active().saturating_sub(1)
            };
            active.set(new);
            scroll_active_into_view(&format!("{popover_id}-opt-{new}"));
        }
    };

    let toggle_value = {
        move |opt: UISelectOption<T>| {
            if opt.disabled {
                return;
            }
            let mut current = value();
            if let Some(pos) = current.iter().position(|v| v == &opt.value) {
                current.remove(pos);
            } else {
                current.push(opt.value);
            }
            value.set(current.clone());
            on_change.call(current);
        }
    };

    rsx! {
        div {
            class: "relative w-full block",
            "data-ui-select-root": true,
            style: "{style}",
            onmounted: move |evt| {
                wrapper_el.set(Some(evt.as_web_event()));
            },

            button {
                r#type: "button",
                id: "{trigger_id}",
                "data-ui-select-trigger": true,
                class: SELECT_TRIGGER_CLASSES,
                disabled,
                "aria-haspopup": "listbox",
                "aria-expanded": is_open(),
                "aria-controls": "{popover_id}",
                "aria-label": name.clone().unwrap_or_default(),
                onclick: move |_| {
                    if !disabled {
                        is_open.set(!is_open());
                    }
                },

                if selected_options.is_empty() {
                    span { class: SELECT_VALUE_PLACEHOLDER_CLASSES, "{placeholder}" }
                } else {
                    span { class: SELECT_CHIPS_CLASSES,
                        for opt in selected_options.iter() {
                            span { class: SELECT_CHIP_CLASSES, key: "chip-{opt.label}",
                                if let Some(rc) = render_chip {
                                    { rc(opt.clone()) }
                                } else {
                                    span { class: SELECT_CHIP_LABEL_CLASSES, "{opt.label}" }
                                }
                                button {
                                    r#type: "button",
                                    class: SELECT_CHIP_REMOVE_CLASSES,
                                    "aria-label": "Remove",
                                    onclick: {
                                        let opt = opt.clone();
                                        let mut toggle = toggle_value;
                                        move |evt: Event<MouseData>| {
                                            evt.stop_propagation();
                                            toggle(opt.clone());
                                        }
                                    },
                                    span { class: "icon-[lucide--x] size-3", "aria-hidden": "true" }
                                }
                            }
                        }
                    }
                }

                span {
                    "data-ui-select-chev": true,
                    class: SELECT_CHEV_CLASSES,
                    "aria-hidden": "true",
                }
            }

            if is_open() {
                div {
                    id: "{popover_id}",
                    "data-ui-select-popover": true,
                    class: pop_class,
                    onkeydown: {
                        let filtered_kd = filtered.clone();
                        let mut move_active_kd = move_active.clone();
                        let mut toggle_kd = toggle_value;
                        let mut close_kd = close.clone();
                        move |evt: KeyboardEvent| {
                            match evt.key() {
                                Key::ArrowDown => { evt.prevent_default(); move_active_kd(1); }
                                Key::ArrowUp => { evt.prevent_default(); move_active_kd(-1); }
                                Key::Enter => {
                                    evt.prevent_default();
                                    if let Some(opt) = filtered_kd.get(active()).cloned() {
                                        toggle_kd(opt);
                                    }
                                }
                                Key::Escape => {
                                    evt.prevent_default();
                                    close_kd();
                                }
                                _ => {}
                            }
                        }
                    },

                    if searchable {
                        div { class: POP_SEARCH_CLASSES,
                            div { class: POP_INPUT_CLASSES,
                                span {
                                    class: "icon-[lucide--search] size-3.5 text-ui-base-muted shrink-0",
                                    "aria-hidden": "true",
                                }
                                input {
                                    id: "{input_id}",
                                    // See note on the matching input in `UISearchSelect`'s popover:
                                    // the wrapper paints the focus halo; opt the input out of the
                                    // global `input:focus-visible` outline to avoid a double ring.
                                    class: POP_INPUT_FIELD_CLASSES,
                                    r#type: "text",
                                    value: "{query}",
                                    placeholder: "{search_placeholder}",
                                    "aria-label": "{search_placeholder}",
                                    oninput: move |evt| {
                                        let q = evt.value();
                                        query.set(q.clone());
                                        if let Some(handler) = on_query {
                                            handler.call(q);
                                        }
                                    },
                                }
                                if !query().is_empty() {
                                    button {
                                        r#type: "button",
                                        class: POP_INPUT_CLEAR_CLASSES,
                                        "aria-label": "Clear search",
                                        onclick: move |_| {
                                            query.set(String::new());
                                            if let Some(handler) = on_query {
                                                handler.call(String::new());
                                            }
                                        },
                                        span { class: "icon-[lucide--x] size-3", "aria-hidden": "true" }
                                    }
                                }
                            }
                        }
                    }

                    if loading {
                        div { class: POP_SKELETON_CLASSES,
                            div { class: POP_SKELETON_ROW_CLASSES }
                            div { class: POP_SKELETON_ROW_CLASSES }
                            div { class: POP_SKELETON_ROW_CLASSES }
                        }
                    } else if filtered.is_empty() {
                        div { class: POP_EMPTY_CLASSES,
                            span {
                                class: "icon-[lucide--inbox] size-5 opacity-50",
                                "aria-hidden": "true",
                            }
                            div { "No options" }
                        }
                    } else {
                        ul { class: POP_LIST_CLASSES,
                            {render_options_list_multi(
                                &filtered,
                                &selected_values,
                                active(),
                                &popover_id,
                                &query(),
                                render_option,
                                &mut active,
                                toggle_value,
                            )}
                        }
                    }

                    if searchable {
                        div { class: POP_FOOT_CLASSES,
                            span { class: POP_HINT_CLASSES,
                                Kbd { label: "↑" }
                                Kbd { label: "↓" }
                                " Navigate"
                            }
                            span { class: POP_HINT_CLASSES,
                                Kbd { label: "↵" }
                                " Toggle"
                            }
                            span { class: POP_FOOT_SPACER_CLASSES }
                            span { class: POP_HINT_CLASSES,
                                Kbd { label: "esc" }
                                " Close"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_options_list_multi<T: Clone + PartialEq + 'static>(
    options: &[UISelectOption<T>],
    selected: &[T],
    active: usize,
    popover_id: &str,
    query: &str,
    render_option: Option<RenderOptionWithQuery<T>>,
    active_signal: &mut Signal<usize>,
    toggle: impl FnMut(UISelectOption<T>) + 'static + Clone,
) -> Element {
    let mut active_signal = *active_signal;
    let mut current_group: Option<String> = None;
    let mut nodes: Vec<Element> = Vec::with_capacity(options.len());

    for (idx, opt) in options.iter().enumerate() {
        let group_changed = opt.group.as_deref() != current_group.as_deref();
        if group_changed {
            if let Some(g) = &opt.group {
                let g = g.clone();
                nodes.push(rsx! {
                    li {
                        key: "{popover_id}-grp-{idx}",
                        class: POP_SECTION_CLASSES,
                        role: "presentation",
                        "{g}"
                    }
                });
            }
            current_group = opt.group.clone();
        }

        let opt_id = format!("{popover_id}-opt-{idx}");
        let is_selected = selected.contains(&opt.value);
        let is_active = active == idx;
        let is_disabled = opt.disabled;
        let opt_for_toggle = opt.clone();
        let opt_for_render = opt.clone();
        let mut toggle = toggle.clone();
        let query_owned = query.to_string();

        nodes.push(rsx! {
            li {
                key: "{opt_id}",
                id: "{opt_id}",
                role: "option",
                "aria-selected": is_selected,
                "aria-disabled": is_disabled,
                "data-active": is_active,
                class: POP_ITEM_CLASSES,
                onmouseenter: move |_| {
                    if !is_disabled {
                        active_signal.set(idx);
                    }
                },
                onclick: move |_| {
                    if !is_disabled {
                        toggle(opt_for_toggle.clone());
                    }
                },

                if let Some(rop) = render_option {
                    { rop((opt_for_render.clone(), query_owned.clone())) }
                } else {
                    span { class: POP_ITEM_LABEL_CLASSES,
                        { ss_highlight(&opt_for_render.label, &query_owned) }
                    }
                    if let Some(meta) = &opt_for_render.meta {
                        span { class: POP_ITEM_META_CLASSES, "{meta}" }
                    }
                }

                if is_selected {
                    span {
                        class: POP_ITEM_CHECK_CLASSES,
                        "aria-hidden": "true",
                    }
                }
            }
        });
    }

    rsx! {
        for node in nodes.into_iter() { { node } }
    }
}
