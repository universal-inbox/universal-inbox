use anyhow::Result;
use dioxus::prelude::*;

use futures_util::StreamExt;
use log::error;
use reqwest::Method;
use url::Url;

use universal_inbox::user::{UserPreferences, UserPreferencesPatch};

use crate::{
    model::UniversalInboxUIModel,
    services::{
        api::{call_api, call_api_and_notify},
        toast_service::ToastCommand,
    },
};

#[derive(Debug)]
pub enum UserPreferencesCommand {
    Refresh,
    Patch(UserPreferencesPatch),
}

pub static USER_PREFERENCES: GlobalSignal<Option<UserPreferences>> = Signal::global(|| None);

pub async fn user_preferences_service(
    mut rx: UnboundedReceiver<UserPreferencesCommand>,
    api_base_url: Url,
    user_preferences: Signal<Option<UserPreferences>>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(UserPreferencesCommand::Refresh) => {
                get_user_preferences(&api_base_url, user_preferences, ui_model).await;
            }
            Some(UserPreferencesCommand::Patch(patch)) => {
                patch_user_preferences(
                    &api_base_url,
                    patch,
                    user_preferences,
                    ui_model,
                    toast_service,
                )
                .await;
            }
            None => {}
        }
    }
}

async fn get_user_preferences(
    api_base_url: &Url,
    mut user_preferences: Signal<Option<UserPreferences>>,
    ui_model: Signal<UniversalInboxUIModel>,
) {
    let result: Result<UserPreferences> = call_api(
        Method::GET,
        api_base_url,
        "users/me/preferences",
        None::<i32>,
        Some(ui_model),
    )
    .await;

    match result {
        Ok(preferences) => {
            *user_preferences.write() = Some(preferences);
        }
        Err(err) => {
            error!("Failed to get user preferences: {err}");
        }
    }
}

async fn patch_user_preferences(
    api_base_url: &Url,
    patch: UserPreferencesPatch,
    mut user_preferences: Signal<Option<UserPreferences>>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    let result: Result<UserPreferences> = call_api_and_notify(
        Method::PATCH,
        api_base_url,
        "users/me/preferences",
        Some(patch),
        Some(ui_model),
        &toast_service,
        "Updating preferences...",
        "Preferences updated",
    )
    .await;

    match result {
        Ok(preferences) => {
            *user_preferences.write() = Some(preferences);
        }
        Err(err) => {
            error!("Failed to update user preferences: {err}");
        }
    }
}
