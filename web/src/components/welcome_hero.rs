#![allow(non_snake_case)]

use dioxus::prelude::*;
use rand::seq::IndexedRandom;

use crate::images::UI_LOGO_SYMBOL_TRANSPARENT;

#[component]
pub fn WelcomeHero(inbox_zero_message: ReadOnlySignal<String>) -> Element {
    let tips = use_memo(|| {
        vec![
            rsx! { span { "• Scroll down notification details by a page with ", kbd { class: "kbd kbd-xs", "Space" }, " key" } },
            rsx! { span { "• Scroll notification details with ", kbd { class: "kbd kbd-xs", "j" }, "(down) and ", kbd { class: "kbd kbd-xs", "k" }, "(up) keys" } },
            rsx! { span { "• Expand notification details with ", kbd { class: "kbd kbd-xs", "e" } } },
            rsx! { span { "• Delete notification with ", kbd { class: "kbd kbd-xs", "d" }, " to maintain inbox zero" } },
            rsx! { span { "• Unsubscribe from future update of the notification with ", kbd { class: "kbd kbd-xs", "u" }, " key" } },
            rsx! { span { "• Snooze notification to the day after with ", kbd { class: "kbd kbd-xs", "s" }, " key" } },
            rsx! { span { "• Convert notifications to tasks with ", kbd { class: "kbd kbd-xs", "p" }, " key" } },
            rsx! { span { "• Convert notifications to tasks with defaults parameters with ", kbd { class: "kbd kbd-xs", "t" }, " key" } },
            rsx! { span { "• Open the notification source with ", kbd { class: "kbd kbd-xs", "Enter" }, " key" } },
            rsx! { span { "• Connect multiple services to centralize all your notifications" } },
            rsx! { span { "• Use keyboard shortcuts: Press ", kbd { class: "kbd kbd-xs", "?" }, " to see all available shortcuts" } },
            rsx! { span { "• Complete a task with ", kbd { class: "kbd kbd-xs", "c" }, " key" } },
            rsx! { span { "• Accept a calendar invitation with ", kbd { class: "kbd kbd-xs", "y" }, " key" } },
            rsx! { span { "• Decline a calendar invitation with ", kbd { class: "kbd kbd-xs", "n" }, " key" } },
        ]
    })();
    // Randomly select 3 tips to display
    let mut rng = rand::rng();
    let selected_tips = tips.choose_multiple(&mut rng, 3);

    rsx! {
        div {
            class: "relative w-full h-full flex flex-col justify-center items-center overflow-hidden",
            div {
                class: "relative z-10 flex flex-col items-center max-w-4xl mx-auto px-4 sm:px-6 text-center hero-mobile-spacing",
                div {
                    class: "mb-6 sm:mb-8 transform transition-all duration-700 hover:scale-105 fade-in",
                    img {
                        class: "opacity-60 dark:opacity-40 w-24 h-24 sm:w-32 sm:h-32 md:w-40 md:h-40 lg:w-48 lg:h-48 filter drop-shadow-lg crisp-edges scale-x-270 scale-y-210",
                        src: "{UI_LOGO_SYMBOL_TRANSPARENT}",
                        alt: "Universal Inbox - Your unified notification center"
                    }
                }
                div {
                    class: "mb-4 sm:mb-6 space-y-3 sm:space-y-4 fade-in-delay",
                    h2 {
                        class: "hero-title-mobile sm:text-4xl lg:text-5xl font-bold leading-tight",
                        span {
                            class: "bg-gradient-to-b from-[#12B1FA] to-primary bg-clip-text text-transparent",
                            "Inbox Zero Achieved! "
                        }
                        "🎉"
                    }
                }
                div {
                    class: "space-y-4",
                    div {
                        class: "text-base-content/60",
                        p { "{inbox_zero_message}" }
                        p { "Meanwhile, enjoy this moment of calm! ✨" }
                    }
                }
                div {
                    class: "mt-12 p-6 bg-base-200/50 rounded-2xl backdrop-blur-sm border border-base-content/10 max-w-2xl",
                    h3 {
                        class: "text-lg font-semibold mb-4 text-base-content/80",
                        "💡 Pro Tips"
                    }
                    ul {
                        class: "text-sm text-base-content/70 space-y-2 text-left",
                        for tip in selected_tips {
                            li { { tip } }
                        }
                    }
                }
            }
        }
    }
}
