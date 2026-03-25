#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::components::ui::{Kbd, KbdSize};

#[component]
pub fn WelcomeHero(inbox_zero_message: ReadSignal<String>) -> Element {
    rsx! {
        div {
            class: "flex flex-1 h-full w-full flex-col items-center \
                    justify-center gap-3.5 px-6 py-12 text-center",
            role: "status",
            "aria-live": "polite",

            svg {
                class: "size-[72px] text-ui-success animate-inbox-zero-pop \
                        motion-reduce:animate-none",
                xmlns: "http://www.w3.org/2000/svg",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                "aria-hidden": "true",
                circle { cx: "12", cy: "12", r: "10", class: "opacity-30" }
                path {
                    class: "[stroke-dasharray:24] [stroke-dashoffset:24] \
                            animate-inbox-zero-draw \
                            motion-reduce:animate-none \
                            motion-reduce:[stroke-dashoffset:0]",
                    d: "m8.5 12.2 2.5 2.5 4.5-5",
                    stroke_width: "2",
                }
            }

            div {
                class: "text-[22px] font-semibold tracking-[-0.02em] \
                        text-ui-base-content",
                "You're all caught up"
            }

            div {
                class: "text-[13.5px] leading-[1.55] text-ui-base-muted \
                        max-w-[420px]",
                "{inbox_zero_message}"
            }

            div {
                class: "mt-2 inline-flex items-center gap-1.5 text-[11px] \
                        text-ui-base-muted",
                "Press "
                Kbd { label: "?".to_string(), size: KbdSize::Xs }
                " to see all shortcuts"
            }
        }
    }
}
