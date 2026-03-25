#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::hooks::use_route;

use crate::{
    components::{
        footer::Footer, sidebar::Sidebar, toast_zone::ToastZone,
        universal_inbox_title::UniversalInboxTitle,
    },
    icons::UILogo,
    model::VERSION_MISMATCH,
    route::Route,
    theme::IS_SIDEBAR_DRAWER_OPEN,
};

#[component]
pub fn NavBarLayout() -> Element {
    let version_mismatch = VERSION_MISMATCH.read();
    let route = use_route::<Route>();
    let show_detail = matches!(
        route,
        Route::NotificationPage { .. } | Route::SyncedTaskPage { .. }
    );
    // `.app-layout` class hook is kept so child elements can target it from
    // their own Tailwind variants (e.g. `max-md:[.app-layout.show-detail_&]:`
    // on `.list-panel` / `.detail-panel` / `.detail-back-btn`). The mobile
    // column stack (`flex-direction: column`) is expressed inline via
    // `max-md:flex-col`.
    let layout_class = if show_detail {
        "app-layout show-detail flex flex-1 min-h-0 overflow-hidden max-md:flex-col"
    } else {
        "app-layout flex flex-1 min-h-0 overflow-hidden max-md:flex-col"
    };

    rsx! {
        div {
            class: "flex flex-col h-screen overflow-hidden",
            a {
                class: "skip-link",
                href: "#main-content",
                "Skip to content"
            }
            if let Some(ref backend_version) = *version_mismatch {
                div {
                    class: "w-full bg-warning text-warning-content px-4 py-2 text-center text-sm flex items-center justify-center gap-2",
                    span {
                        "A new version ({backend_version}) is available but could not be loaded automatically. Please hard-refresh your browser (Ctrl+Shift+R / Cmd+Shift+R) or clear your cache."
                    }
                }
            }
            div {
                class: layout_class,
                div {
                    // Drawer backdrop. KEEP_CUSTOM `.sidebar-backdrop.open`
                    // because the `@keyframes backdrop-in` fade-in animation
                    // and the dark `rgba(15,23,42,0.45)` overlay color are
                    // anchored to that class hook. `hidden` keeps it out of
                    // the layout above 1024px where the drawer is inactive.
                    // `max-lg:block` reveals it together with the drawer
                    // breakpoint (was `max-md:` at <=768px).
                    class: if *IS_SIDEBAR_DRAWER_OPEN.read() { "sidebar-backdrop open hidden max-lg:block" } else { "sidebar-backdrop hidden" },
                    onclick: move |_| {
                        if *IS_SIDEBAR_DRAWER_OPEN.read() {
                            *IS_SIDEBAR_DRAWER_OPEN.write() = false;
                        }
                    },
                }
                Sidebar {}
                div {
                    class: "flex flex-col flex-1 min-w-0 overflow-hidden",
                    header {
                        // Drawer-mode topbar (hamburger + brand): `hidden
                        // max-lg:flex` reveals it at <=1024px to pair with
                        // the sidebar drawer breakpoint. Was `max-md:`
                        // (<=768) prior to the tablet-drawer shift. Shell
                        // utilities below mirror the previous `.mobile-topbar`
                        // rule.
                        class: "hidden max-lg:flex max-lg:items-center max-lg:gap-2.5 max-lg:h-12 max-lg:px-2.5 max-lg:bg-ui-surface max-lg:border-b max-lg:border-ui-border max-lg:flex-shrink-0",
                        button {
                            // Hamburger: utility-only. No `::before`
                            // decoration so safe to drop the class hook.
                            class: "w-10 h-10 inline-flex items-center justify-center bg-transparent border-0 rounded-ui-sm text-ui-base-content cursor-pointer hover:bg-ui-surface-hover",
                            "aria-label": "Open navigation",
                            "aria-expanded": "{*IS_SIDEBAR_DRAWER_OPEN.read()}",
                            onclick: move |_| {
                                let cur = *IS_SIDEBAR_DRAWER_OPEN.read();
                                *IS_SIDEBAR_DRAWER_OPEN.write() = !cur;
                            },
                            span { class: "icon-[lucide--menu] size-5" }
                        }
                        UILogo { class: "w-[22px] h-[26px] flex-shrink-0" }
                        UniversalInboxTitle {}
                    }
                    main {
                        id: "main-content",
                        class: "flex-1 overflow-hidden flex flex-col",
                        role: "main",
                        Outlet::<Route> {}
                    }
                    Footer {}
                }
            }
            ToastZone {}
        }
    }
}
