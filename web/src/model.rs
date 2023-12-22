use fermi::AtomRef;

pub static UI_MODEL: AtomRef<UniversalInboxUIModel> = AtomRef(|_| Default::default());

#[derive(Debug, Default)]
pub struct UniversalInboxUIModel {
    pub selected_notification_index: usize,
    pub is_help_enabled: bool,
    pub is_task_actions_enabled: bool,
    pub task_planning_modal_opened: bool,
    pub task_link_modal_opened: bool,
    pub unhover_element: bool,
    pub authentication_state: AuthenticationState,
    pub loaded_notifications: Option<Result<usize, String>>,
    pub selected_preview_pane: PreviewPane,
    pub error_message: Option<String>,
}

impl UniversalInboxUIModel {
    pub fn toggle_help(&mut self) {
        self.is_help_enabled = !self.is_help_enabled;
    }

    pub fn set_unhover_element(&mut self, unhover_element: bool) -> bool {
        if self.unhover_element != unhover_element {
            self.unhover_element = unhover_element;
            return true;
        }
        false
    }
}

#[derive(Debug, PartialEq, Default, Clone)]
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
