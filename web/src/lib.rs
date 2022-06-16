use ::universal_inbox::Notification;
use reqwasm::http::Request;
use uikit_rs as uk;
use wasm_bindgen::prelude::*;
use yew::prelude::*;

mod components;

#[wasm_bindgen(module = "/js/api.js")]
extern "C" {
    fn get_api_base_url() -> String;
}

#[function_component(App)]
pub fn app() -> Html {
    let notifications = use_state(|| None);
    {
        use_effect_with_deps(
            move |_| {
                wasm_bindgen_futures::spawn_local(async move {
                    let fetched_notifications: Vec<Notification> =
                        Request::get(&format!("{}/notifications", get_api_base_url()))
                            .send()
                            .await
                            .unwrap()
                            .json()
                            .await
                            .unwrap();
                    notifications.set(Some(fetched_notifications));
                });
                || ()
            },
            (),
        );
    }

    html! {
        <uk::Section style={uk::SectionStyle::Default}>
          <uk::Container size={uk::ContainerSize::Small}>

          </uk::Container>
        </uk::Section>
    }
}
