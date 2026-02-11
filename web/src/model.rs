use dioxus::prelude::*;

use crate::{
    services::local_storage::LocalStorageService,
    settings::{PanelPosition, UserSettings},
};

pub static UI_MODEL: GlobalSignal<UniversalInboxUIModel> = Signal::global(|| {
    let settings = LocalStorageService::load_settings();
    UniversalInboxUIModel::from_settings(settings)
});
pub const DEFAULT_USER_AVATAR: &str = "https://avatars.githubusercontent.com/u/1062408?v=4";
pub const VERSION: Option<&'static str> = option_env!("VERSION");
pub static VERSION_MISMATCH: GlobalSignal<Option<String>> = Signal::global(|| None);

#[derive(Debug)]
pub struct UniversalInboxUIModel {
    pub selected_notification_index: Option<usize>,
    pub selected_task_index: Option<usize>,
    pub is_help_enabled: bool,
    pub is_task_actions_enabled: bool,
    pub authentication_state: AuthenticationState,
    pub selected_preview_pane: PreviewPane,
    pub error_message: Option<String>,
    pub confirmation_message: Option<String>,
    pub is_syncing_notifications: bool,
    pub is_syncing_tasks: bool,
    pub preview_cards_expanded: bool,
    details_panel_width: f64,
    details_panel_height: f64,
    details_panel_position: PanelPosition,
}

impl Default for UniversalInboxUIModel {
    fn default() -> Self {
        let settings = LocalStorageService::load_settings();
        Self::from_settings(settings)
    }
}

impl UniversalInboxUIModel {
    pub fn from_settings(settings: UserSettings) -> Self {
        Self {
            selected_notification_index: None,
            selected_task_index: None,
            is_help_enabled: false,
            is_task_actions_enabled: false,
            authentication_state: AuthenticationState::default(),
            selected_preview_pane: PreviewPane::default(),
            error_message: None,
            confirmation_message: None,
            is_syncing_notifications: false,
            is_syncing_tasks: false,
            preview_cards_expanded: false,
            details_panel_width: settings.ui.details_panel_width,
            details_panel_height: settings.ui.details_panel_height,
            details_panel_position: settings.ui.details_panel_position.clone(),
        }
    }

    pub fn toggle_help(&mut self) {
        self.is_help_enabled = !self.is_help_enabled;
    }

    pub fn toggle_preview_cards(&mut self) {
        self.preview_cards_expanded = !self.preview_cards_expanded;
    }

    pub fn set_details_panel_width(&mut self, width: f64) {
        let clamped_width = width.clamp(20.0, 60.0);
        self.details_panel_width = clamped_width;

        // Save to local storage
        LocalStorageService::update_ui_setting(|ui_settings| {
            ui_settings.details_panel_width = clamped_width;
        });
    }

    pub fn get_details_panel_width(&self) -> f64 {
        self.details_panel_width
    }

    pub fn set_details_panel_height(&mut self, height: f64) {
        let clamped_height = height.clamp(20.0, 80.0);
        self.details_panel_height = clamped_height;

        // Save to local storage
        LocalStorageService::update_ui_setting(|ui_settings| {
            ui_settings.details_panel_height = clamped_height;
        });
    }

    pub fn get_details_panel_height(&self) -> f64 {
        self.details_panel_height
    }

    pub fn set_details_panel_position(&mut self, position: PanelPosition) {
        self.details_panel_position = position.clone();

        // Save to local storage
        LocalStorageService::update_ui_setting(|ui_settings| {
            ui_settings.details_panel_position = position;
        });
    }

    pub fn get_details_panel_position(&self) -> &PanelPosition {
        &self.details_panel_position
    }

    pub fn toggle_details_panel_position(&mut self) {
        let new_position = match self.details_panel_position {
            PanelPosition::Right => PanelPosition::Bottom,
            PanelPosition::Bottom => PanelPosition::Right,
        };
        self.set_details_panel_position(new_position);
    }
}

#[derive(Debug, PartialEq, Default, Clone, Copy)]
pub enum AuthenticationState {
    // When we don't know if the user is authenticated, we will load the application as if
    // she is authenticated and deduce the state from the first API request result
    #[default]
    Unknown,
    NotAuthenticated,
    RedirectingToAuthProvider,
    FetchingAccessToken,
    VerifyingAccessToken,
    Authenticated,
}

impl AuthenticationState {
    pub fn label(&self) -> String {
        match self {
            AuthenticationState::Authenticated => "Authenticated",
            AuthenticationState::NotAuthenticated => "Authenticating...",
            AuthenticationState::RedirectingToAuthProvider => "Redirecting to login...",
            AuthenticationState::FetchingAccessToken => "Authenticating...",
            AuthenticationState::VerifyingAccessToken => "Authenticating session...",
            AuthenticationState::Unknown => "...",
        }
        .to_string()
    }
}

#[derive(Debug, Default, PartialEq)]
pub enum PreviewPane {
    #[default]
    Notification,
    Task,
}

#[derive(Debug, PartialEq, Clone)]
pub enum LoadState<T> {
    None,
    Loading,
    Loaded(T),
    Error(String),
}
