#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::hooks::use_route;
use gravatar_rs::Generator;

use crate::{
    components::{
        flyonui::tooltip::{Tooltip, TooltipPlacement},
        ui::{
            Badge, BadgeVariant, NavItem, NavSection,
            nav_item::{NAV_ICON_BASE, NAV_LINK_ACTIVE, NAV_LINK_BASE},
        },
        universal_inbox_title::UniversalInboxTitle,
    },
    icons::UILogo,
    model::DEFAULT_USER_AVATAR,
    route::Route,
    services::{
        notification_service::NOTIFICATIONS_PAGE,
        task_service::SYNCED_TASKS_PAGE,
        user_service::{CONNECTED_USER, UserCommand},
    },
    theme::{
        IS_DARK_MODE, IS_SIDEBAR_COLLAPSED, IS_SIDEBAR_DRAWER_OPEN, toggle_dark_mode,
        toggle_sidebar_collapsed,
    },
};

#[component]
pub fn Sidebar() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let route = use_route::<Route>();
    let notifications_active = matches!(
        route,
        Route::NotificationsPage {} | Route::NotificationPage { .. }
    );
    let synced_tasks_active = matches!(
        route,
        Route::SyncedTasksPage {} | Route::SyncedTaskPage { .. }
    );

    use_effect(use_reactive!(|route| {
        let _ = route;
        if *IS_SIDEBAR_DRAWER_OPEN.peek() {
            *IS_SIDEBAR_DRAWER_OPEN.write() = false;
        }
    }));

    // The `sidebar` class hook stays on the <aside> because the
    // `.sidebar.collapsed .sidebar-brand` cascade still binds to it via raw
    // CSS (the cascade reshapes the brand row's flex direction + padding —
    // hard to express as utility composition). The `collapsed` and `open`
    // state classes are toggled here; child elements use Tailwind variants
    // like `md:[.sidebar.collapsed_&]:hidden` to react to them.
    //
    // The `max-lg:*` variants below express the tablet/mobile drawer behavior
    // (was `max-md:` while the breakpoint was 768px — raised to <=1024px so
    // the sidebar collapses to a drawer earlier, while the list+detail
    // two-pane layout still renders side-by-side down to 769px). The sidebar
    // becomes a fixed-position 280px-wide overlay translated off-screen
    // (`-translate-x-full`), which slides into view when the `.open` class
    // is toggled. `max-lg:[&.collapsed]:` also defeats the desktop collapsed
    // rail (`md:[&.collapsed]:w-14`) so the drawer always renders full-width.
    let shell_class = "sidebar flex flex-col w-56 min-w-56 bg-sidebar-bg border-r border-sidebar-border md:[&.collapsed]:w-14 md:[&.collapsed]:min-w-14 max-lg:fixed max-lg:top-0 max-lg:left-0 max-lg:h-screen max-lg:w-[280px] max-lg:min-w-[280px] max-lg:z-[1000] max-lg:-translate-x-full max-lg:transition-transform max-lg:duration-[250ms] max-lg:ease-out max-lg:shadow-ui-lg max-lg:[&.open]:translate-x-0 max-lg:[&.collapsed]:w-[280px] max-lg:[&.collapsed]:min-w-[280px]";
    let sidebar_class = match (*IS_SIDEBAR_COLLAPSED.read(), *IS_SIDEBAR_DRAWER_OPEN.read()) {
        (true, true) => format!("{shell_class} collapsed open"),
        (true, false) => format!("{shell_class} collapsed"),
        (false, true) => format!("{shell_class} open"),
        (false, false) => shell_class.to_string(),
    };
    let user_avatar = use_memo(|| {
        CONNECTED_USER()
            .as_ref()
            .map(|user| {
                if let Some(ref email) = user.email {
                    Generator::default()
                        .set_image_size(150)
                        .set_rating("g")
                        .set_default_image("mp")
                        .generate(email.as_str())
                } else {
                    DEFAULT_USER_AVATAR.to_string()
                }
            })
            .unwrap_or_else(|| DEFAULT_USER_AVATAR.to_string())
    });

    rsx! {
        aside {
            class: "{sidebar_class}",
            role: "navigation",
            "aria-label": "Main navigation",

            // Brand
            // NOTE: `.sidebar-brand` stays as a class hook because the
            // `.sidebar.collapsed .sidebar-brand` cascade reshapes the
            // layout (flex-column, smaller logo) and the mobile @media
            // block reverts it. Both rules still live in
            // `web/css/universal-inbox.css`.
            div { class: "sidebar-brand flex items-center gap-2.5 pt-3.5 pb-2.5 px-3.5 border-b border-sidebar-border",
                UILogo { class: "logo-icon" }
                span { class: "brand-name text-sm tracking-tight md:[.sidebar.collapsed_&]:hidden", UniversalInboxTitle {} }
                // NOTE: kept as a bare <button class="sidebar-collapse-btn">
                // because `<Button variant=Icon>` emits `hdr-btn` styling
                // (border + surface bg) which doesn't match the borderless
                // sidebar-text appearance. The `.sidebar.collapsed
                // .sidebar-collapse-btn { margin-left: 0 }` rule also binds
                // to this class hook for the collapsed-state layout.
                button {
                    // `.sidebar-collapse-btn` class hook kept for the CSS
                    // shell styling (border, hover bg, size, chevron color)
                    // and the `.sidebar.collapsed .sidebar-collapse-btn`
                    // margin reset. `max-lg:hidden!` hides the chevron inside
                    // the drawer (active at <=1024px) where collapse has no
                    // meaning. The `!important` (`!` suffix in Tailwind v4)
                    // is required because the `.sidebar-collapse-btn { display:
                    // flex }` rule declared later in the cascade has equal
                    // specificity and would otherwise win.
                    class: "sidebar-collapse-btn max-lg:hidden!",
                    "aria-label": if *IS_SIDEBAR_COLLAPSED.read() { "Expand sidebar" } else { "Collapse sidebar" },
                    "aria-pressed": "{*IS_SIDEBAR_COLLAPSED.read()}",
                    onclick: move |_| {
                        let current = *IS_SIDEBAR_COLLAPSED.read();
                        *IS_SIDEBAR_COLLAPSED.write() =
                            toggle_sidebar_collapsed(current).expect("Failed to toggle sidebar");
                    },
                    span {
                        class: if *IS_SIDEBAR_COLLAPSED.read() {
                            "icon-[lucide--chevrons-right] size-4"
                        } else {
                            "icon-[lucide--chevrons-left] size-4"
                        }
                    }
                }
            }

            // Navigation
            nav { class: "flex-1 overflow-y-auto py-2 px-1.5 md:[.sidebar.collapsed_&]:overflow-visible",
                NavSection { label: "Notifications".to_string(),
                    // Inbox and Synced tasks keep manual <Link> markup because
                    // their active state spans multiple routes (NotificationsPage
                    // OR NotificationPage). They compose the same Tailwind
                    // class constants as <NavItem> so styling stays in lockstep.
                    Tooltip {
                        class: "block",
                        text: "Inbox",
                        placement: TooltipPlacement::Right,
                        disabled: !*IS_SIDEBAR_COLLAPSED.read(),
                        Link {
                            class: if notifications_active { "{NAV_LINK_BASE} {NAV_LINK_ACTIVE}" } else { "{NAV_LINK_BASE}" },
                            to: Route::NotificationsPage {},
                            span { class: "icon-[lucide--inbox] size-4 {NAV_ICON_BASE}" }
                            span { class: "md:[.sidebar.collapsed_&]:hidden", "Inbox" }
                            if NOTIFICATIONS_PAGE().total > 0 {
                                span { class: "ml-auto md:[.sidebar.collapsed_&]:hidden",
                                    Badge { variant: BadgeVariant::Primary, "{NOTIFICATIONS_PAGE().total}" }
                                }
                            }
                        }
                    }
                }

                NavSection { label: "Tasks".to_string(),
                    Tooltip {
                        class: "block",
                        text: "Synced tasks",
                        placement: TooltipPlacement::Right,
                        disabled: !*IS_SIDEBAR_COLLAPSED.read(),
                        Link {
                            class: if synced_tasks_active { "{NAV_LINK_BASE} {NAV_LINK_ACTIVE}" } else { "{NAV_LINK_BASE}" },
                            to: Route::SyncedTasksPage {},
                            span { class: "icon-[lucide--bookmark-check] size-4 {NAV_ICON_BASE}" }
                            span { class: "md:[.sidebar.collapsed_&]:hidden", "Synced tasks" }
                            if SYNCED_TASKS_PAGE().total > 0 {
                                span { class: "ml-auto md:[.sidebar.collapsed_&]:hidden",
                                    Badge { variant: BadgeVariant::Muted, "{SYNCED_TASKS_PAGE().total}" }
                                }
                            }
                        }
                    }
                }

                NavSection { label: "Manage".to_string(),
                    Tooltip {
                        class: "block",
                        text: "Settings",
                        placement: TooltipPlacement::Right,
                        disabled: !*IS_SIDEBAR_COLLAPSED.read(),
                        NavItem {
                            icon_class: "icon-[lucide--settings] size-4".to_string(),
                            label: "Settings".to_string(),
                            to: Route::SettingsPage {},
                        }
                    }

                    Tooltip {
                        class: "block",
                        text: "Security",
                        placement: TooltipPlacement::Right,
                        disabled: !*IS_SIDEBAR_COLLAPSED.read(),
                        NavItem {
                            icon_class: "icon-[lucide--shield] size-4".to_string(),
                            label: "Security".to_string(),
                            to: Route::SecurityPage {},
                        }
                    }
                }
            }

            // Footer with user profile and actions
            div { class: "py-2 px-1.5 border-t border-sidebar-border md:[.sidebar.collapsed_&]:py-2 md:[.sidebar.collapsed_&]:px-1",
                // Dark mode toggle
                // NOTE: the inner `<span class="ui-toggle">` is kept as a
                // non-interactive visual indicator. Swapping it for
                // <ToggleSwitch size=Sm> would nest a <label><input/></label>
                // inside this <button>, which is invalid HTML (nested
                // interactive elements). The parent button owns the click.
                Tooltip {
                    class: "block",
                    text: if *IS_DARK_MODE.read() { "Switch to light mode" } else { "Switch to dark mode" },
                    placement: TooltipPlacement::Right,
                    disabled: !*IS_SIDEBAR_COLLAPSED.read(),
                    button {
                        class: "flex items-center justify-between w-full py-1 px-2.5 rounded-ui-md bg-transparent border-0 mb-px font-ui text-[12.5px] text-sidebar-text cursor-pointer transition-colors duration-[120ms] hover:bg-sidebar-hover-bg hover:text-sidebar-text-bright md:[.sidebar.collapsed_&]:justify-center",
                        "aria-label": "Toggle dark mode",
                        "aria-pressed": "{*IS_DARK_MODE.read()}",
                        onclick: move |_| {
                            *IS_DARK_MODE.write() = toggle_dark_mode(true).expect("Failed to toggle theme");
                        },
                        span { class: "flex items-center gap-2",
                            if *IS_DARK_MODE.read() {
                                span { class: "icon-[lucide--moon] size-4 text-base opacity-65 text-sidebar-text-muted shrink-0" }
                                span { class: "md:[.sidebar.collapsed_&]:hidden", "Dark mode" }
                            } else {
                                span { class: "icon-[lucide--sun] size-4 text-base opacity-65 text-sidebar-text-muted shrink-0" }
                                span { class: "md:[.sidebar.collapsed_&]:hidden", "Light mode" }
                            }
                        }
                        span { class: if *IS_DARK_MODE.read() { "ui-toggle on md:[.sidebar.collapsed_&]:hidden" } else { "ui-toggle md:[.sidebar.collapsed_&]:hidden" } }
                    }
                }

                // User profile — links to the profile page
                Tooltip {
                    class: "block",
                    text: "Profile",
                    placement: TooltipPlacement::Right,
                    disabled: !*IS_SIDEBAR_COLLAPSED.read(),
                    Link {
                        class: "flex items-center gap-2 py-1.5 px-2 rounded-ui-md cursor-pointer hover:bg-sidebar-hover-bg md:[.sidebar.collapsed_&]:justify-center",
                        to: Route::UserProfilePage {},
                        img {
                            class: "w-7 h-7 rounded-full bg-ui-primary flex items-center justify-center text-[10px] font-semibold text-ui-on-brand shrink-0",
                            src: "{user_avatar()}",
                            alt: "{CONNECTED_USER().as_ref().and_then(|u| u.full_name()).unwrap_or_default()}'s avatar",
                        }
                        div { class: "flex flex-col min-w-0 md:[.sidebar.collapsed_&]:hidden",
                            if let Some(user) = CONNECTED_USER().as_ref() {
                                if let Some(name) = user.full_name() {
                                    span { class: "text-xs font-medium text-sidebar-text-bright", "{name}" }
                                }
                                if let Some(ref email) = user.email {
                                    span { class: "text-[10px] text-sidebar-text-muted overflow-hidden text-ellipsis whitespace-nowrap", "{email}" }
                                }
                            }
                        }
                    }
                }

                // Logout
                Tooltip {
                    class: "block",
                    text: "Logout",
                    placement: TooltipPlacement::Right,
                    disabled: !*IS_SIDEBAR_COLLAPSED.read(),
                    button {
                        class: "{NAV_LINK_BASE} w-full",
                        onclick: move |_| user_service.send(UserCommand::Logout),
                        span { class: "icon-[lucide--log-out] size-4 {NAV_ICON_BASE}" }
                        span { class: "md:[.sidebar.collapsed_&]:hidden", "Logout" }
                    }
                }
            }
        }
    }
}
