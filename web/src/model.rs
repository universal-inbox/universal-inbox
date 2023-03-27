use fermi::AtomRef;

pub static UI_MODEL: AtomRef<UniversalInboxUIModel> = |_| Default::default();

#[derive(Debug, Default)]
pub struct UniversalInboxUIModel {
    pub selected_notification_index: usize,
    pub footer_help_opened: bool,
    pub task_planning_modal_opened: bool,
    pub task_association_modal_opened: bool,
    pub unhover_element: bool,
    pub authentication_state: AuthenticationState,
}

impl UniversalInboxUIModel {
    pub fn toggle_help(&mut self) {
        self.footer_help_opened = !self.footer_help_opened;
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
