#![allow(non_snake_case)]

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use crate::{model::UI_MODEL, settings::PanelPosition, utils::get_screen_width};

#[component]
pub fn ResizablePanel(children: Element) -> Element {
    let position = UI_MODEL.read().get_details_panel_position().clone();
    let mut is_resizing = use_signal(|| false);
    let mut resize_start_x = use_signal(|| 0.0);
    let mut resize_start_y = use_signal(|| 0.0);
    let mut resize_start_width = use_signal(|| 0.0);
    let mut resize_start_height = use_signal(|| 0.0);
    let width_style = use_memo(|| {
        if get_screen_width().map(|w| w >= 1024).unwrap_or(true) {
            format!("width: {}%;", UI_MODEL.read().get_details_panel_width())
        } else {
            String::new()
        }
    });

    let height_style = use_memo(|| {
        if get_screen_width().map(|w| w >= 1024).unwrap_or(true) {
            format!("height: {}%;", UI_MODEL.read().get_details_panel_height())
        } else {
            String::new()
        }
    });

    // Set up global mouse event listeners when resizing
    use_effect(move || {
        if *is_resizing.read() {
            let document = web_sys::window().unwrap().document().unwrap();

            // let is_resizing_clone = is_resizing.clone();
            // let resize_start_x_clone = resize_start_x.clone();
            // let resize_start_width_clone = resize_start_width.clone();

            let mousemove_closure =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
                    if *is_resizing.read() {
                        let position = UI_MODEL.read().get_details_panel_position().clone();

                        match position {
                            PanelPosition::Right => {
                                let delta_x = event.client_x() as f64 - *resize_start_x.read();
                                let parent_width = web_sys::window()
                                    .and_then(|w| w.inner_width().ok())
                                    .and_then(|w| w.as_f64())
                                    .unwrap_or(1200.0);

                                let delta_percent = (delta_x / parent_width) * 100.0;
                                let new_width =
                                    (*resize_start_width.read() - delta_percent).clamp(20.0, 60.0);

                                log::debug!("Resizing panel to width: {}%", new_width);
                                UI_MODEL.write().set_details_panel_width(new_width);
                            }
                            PanelPosition::Bottom => {
                                let delta_y = event.client_y() as f64 - *resize_start_y.read();
                                let parent_height = web_sys::window()
                                    .and_then(|w| w.inner_height().ok())
                                    .and_then(|h| h.as_f64())
                                    .unwrap_or(800.0);

                                let delta_percent = (delta_y / parent_height) * 100.0;
                                let new_height =
                                    (*resize_start_height.read() - delta_percent).clamp(20.0, 80.0);

                                log::debug!("Resizing panel to height: {}%", new_height);
                                UI_MODEL.write().set_details_panel_height(new_height);
                            }
                        }
                    }
                }) as Box<dyn FnMut(_)>);

            //let mut is_resizing_clone2 = is_resizing.clone();
            let mouseup_closure =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |_event: web_sys::MouseEvent| {
                    is_resizing.set(false);
                }) as Box<dyn FnMut(_)>);

            document
                .add_event_listener_with_callback(
                    "mousemove",
                    mousemove_closure.as_ref().unchecked_ref(),
                )
                .ok();
            document
                .add_event_listener_with_callback(
                    "mouseup",
                    mouseup_closure.as_ref().unchecked_ref(),
                )
                .ok();

            mousemove_closure.forget();
            mouseup_closure.forget();
        }
    });

    match position {
        PanelPosition::Right => rsx! {
            div {
                class: "h-full max-lg:w-full max-lg:absolute flex flex-row bg-base-100 z-auto min-w-1/5 lg:max-w-3/5",
                style: "{width_style()}",

                // Resize handle
                div {
                    class: "w-1 bg-base-content/20 hover:bg-primary cursor-col-resize flex-shrink-0 group relative max-lg:hidden",

                    onmousedown: move |evt: Event<MouseData>| {
                        is_resizing.set(true);
                        resize_start_x.set(evt.client_coordinates().x);
                        resize_start_width.set(UI_MODEL.read().get_details_panel_width());
                    },

                    // Visual indicator
                    div {
                        class: "absolute inset-y-0 -left-1 -right-1 group-hover:bg-primary/20 transition-colors"
                    }

                    // Center grip dots
                    div {
                        class: "absolute top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity",
                        div {
                            class: "w-0.5 h-1 bg-primary mb-0.5 rounded-full"
                        }
                        div {
                            class: "w-0.5 h-1 bg-primary mb-0.5 rounded-full"
                        }
                        div {
                            class: "w-0.5 h-1 bg-primary rounded-full"
                        }
                    }
                }

                // Content
                div {
                    class: "flex-1 px-2 py-2 relative",

                    // Position toggle button
                    button {
                        class: "absolute top-2 right-2 z-10 btn btn-circle btn-xs btn-ghost hover:btn-primary opacity-50 hover:opacity-100 transition-opacity",
                        title: "Switch to bottom panel",
                        onclick: move |_| {
                            UI_MODEL.write().toggle_details_panel_position();
                        },
                        span { class: "icon-[tabler--layout-sidebar-right-collapse] size-4" }
                    }

                    {children}
                }
            }
        },
        PanelPosition::Bottom => rsx! {
            div {
                class: "w-full bg-base-100 z-auto flex flex-col",
                style: "{height_style()}",

                // Resize handle
                div {
                    class: "h-1 bg-base-content/20 hover:bg-primary cursor-row-resize flex-shrink-0 group relative max-lg:hidden",

                    onmousedown: move |evt: Event<MouseData>| {
                        is_resizing.set(true);
                        resize_start_y.set(evt.client_coordinates().y);
                        resize_start_height.set(UI_MODEL.read().get_details_panel_height());
                    },

                    // Visual indicator
                    div {
                        class: "absolute inset-x-0 -top-1 -bottom-1 group-hover:bg-primary/20 transition-colors"
                    }

                    // Center grip dots
                    div {
                        class: "absolute top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity flex",
                        div {
                            class: "w-1 h-0.5 bg-primary mr-0.5 rounded-full"
                        }
                        div {
                            class: "w-1 h-0.5 bg-primary mr-0.5 rounded-full"
                        }
                        div {
                            class: "w-1 h-0.5 bg-primary rounded-full"
                        }
                    }
                }

                // Content
                div {
                    class: "flex-1 px-2 py-2 relative overflow-y-auto",

                    // Position toggle button
                    button {
                        class: "absolute top-2 right-2 z-10 btn btn-circle btn-xs btn-ghost hover:btn-primary opacity-50 hover:opacity-100 transition-opacity",
                        title: "Switch to right panel",
                        onclick: move |_| {
                            UI_MODEL.write().toggle_details_panel_position();
                        },
                        span { class: "icon-[tabler--layout-sidebar-right-expand] size-4" }
                    }

                    {children}
                }
            }
        },
    }
}
