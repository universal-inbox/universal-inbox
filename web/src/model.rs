use dioxus::prelude::*;

pub static UI_MODEL: GlobalSignal<UniversalInboxUIModel> = Signal::global(Default::default);
pub const DEFAULT_USER_AVATAR: &str = "https://avatars.githubusercontent.com/u/1062408?v=4";
pub const VERSION: Option<&'static str> = option_env!("VERSION");

#[derive(Debug, Default)]
pub struct UniversalInboxUIModel {
    pub selected_notification_index: usize,
    pub selected_task_index: usize,
    pub is_help_enabled: bool,
    pub is_task_actions_enabled: bool,
    pub task_planning_modal_opened: bool,
    pub task_link_modal_opened: bool,
    pub authentication_state: AuthenticationState,
    pub selected_preview_pane: PreviewPane,
    pub error_message: Option<String>,
    pub confirmation_message: Option<String>,
    pub is_syncing_notifications: bool,
    pub is_syncing_tasks: bool,
    pub preview_cards_expanded: bool,
}

impl UniversalInboxUIModel {
    pub fn toggle_help(&mut self) {
        self.is_help_enabled = !self.is_help_enabled;
    }

    pub fn toggle_preview_cards(&mut self) {
        self.preview_cards_expanded = !self.preview_cards_expanded;
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

#[derive(Debug)]
pub enum LoadState<T> {
    None,
    Loading,
    Loaded(T),
    Error(String),
}
