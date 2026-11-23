//! What the scenarios of the account, session and email features keep between their steps.

use std::fmt;

use email_address::EmailAddress;
use reqwest::Client;
use url::Url;

use universal_inbox::{
    pii::Pii,
    user::{EmailValidationToken, PasswordResetToken, User, UserAuthMethod, UserId},
};

use crate::helpers::user::CopiedSession;

pub fn ordinal(word: &str) -> usize {
    match word {
        "first" => 0,
        "second" => 1,
        "third" => 2,
        _ => panic!("Unknown ordinal {word:?}"),
    }
}

/// The account the scenario signs in with.
#[derive(Debug, Default)]
pub struct Account {
    email: Option<Pii<EmailAddress>>,
    password: Option<String>,
    user: Option<User>,
    unregistered: Option<Pii<EmailAddress>>,
}

impl Account {
    pub fn set(&mut self, email: Pii<EmailAddress>, password: &str, user: User) {
        self.email = Some(email);
        self.password = Some(password.to_string());
        self.user = Some(user);
    }

    /// An email address and password nobody registered.
    pub fn set_without_account(&mut self, email: Pii<EmailAddress>, password: &str) {
        self.email = Some(email);
        self.password = Some(password.to_string());
    }

    pub fn set_unregistered(&mut self, email: Pii<EmailAddress>) {
        self.unregistered = Some(email);
    }

    pub fn unregistered(&self) -> Pii<EmailAddress> {
        self.unregistered
            .clone()
            .expect("No unregistered email address was chosen in this scenario")
    }

    pub fn email(&self) -> Pii<EmailAddress> {
        self.email
            .clone()
            .expect("No account was created in this scenario")
    }

    pub fn password(&self) -> &str {
        self.password
            .as_deref()
            .expect("No account was created in this scenario")
    }

    pub fn user(&self) -> &User {
        self.user
            .as_ref()
            .expect("No account was created in this scenario")
    }
}

/// A browser: an HTTP client that keeps its cookies, started on first use.
#[derive(Debug, Default)]
pub struct Browser {
    client: Option<Client>,
}

impl Browser {
    pub fn client(&mut self) -> &Client {
        self.client.get_or_insert_with(cookie_client)
    }
}

pub fn cookie_client() -> Client {
    Client::builder()
        .cookie_store(true)
        .build()
        .expect("Failed to build the HTTP client")
}

/// The sessions the account opened by signing in with its password, in order.
#[derive(Default)]
pub struct Sessions {
    opened: Vec<CopiedSession>,
    /// The API token the user created, to use as a bearer token.
    pub api_token: Option<String>,
}

impl fmt::Debug for Sessions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} session(s)", self.opened.len())
    }
}

impl Sessions {
    pub fn open(&mut self, session: CopiedSession) {
        self.opened.push(session);
    }

    pub fn nth(&self, ordinal: &str) -> &CopiedSession {
        self.opened
            .get(self::ordinal(ordinal))
            .unwrap_or_else(|| panic!("The {ordinal} session was not opened"))
    }

    pub fn latest(&self) -> &CopiedSession {
        self.opened
            .last()
            .expect("No session was opened in this scenario")
    }
}

/// The addresses the identity provider was asked to sign in at.
#[derive(Debug, Default)]
pub struct Authorizations {
    pub urls: Vec<Url>,
}

/// What linking an authentication method through the app's services answered.
#[derive(Debug, Default)]
pub struct LinkedMethod {
    pub result: Option<Result<UserAuthMethod, String>>,
}

/// What the scenario knows about the verification of the account's email address.
#[derive(Debug, Default)]
pub struct EmailVerification {
    /// The verification token the account had before a new email was asked for.
    pub previous_token: Option<EmailValidationToken>,
    /// The verification link that was last opened.
    pub link: Option<String>,
    /// The user a verification link named, when it is not the account's.
    pub link_user_id: Option<UserId>,
}

/// What the scenario knows about the password reset of the account.
#[derive(Debug, Default)]
pub struct PasswordReset {
    /// The token of the last reset email.
    pub token: Option<PasswordResetToken>,
    /// The user a reset link named, when it is not the account's.
    pub link_user_id: Option<UserId>,
}
