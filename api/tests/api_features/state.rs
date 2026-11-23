//! What the scenarios of the authentication features keep between their steps.

use std::fmt;

use email_address::EmailAddress;
use reqwest::Client;
use serde_json::Value;
use url::Url;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
use webauthn_rs::prelude::{RegisterPublicKeyCredential, RequestChallengeResponse};

use universal_inbox::{
    pii::Pii,
    user::{User, UserAuthMethod},
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
}

impl Account {
    pub fn set(&mut self, email: Pii<EmailAddress>, password: &str, user: User) {
        self.email = Some(email);
        self.password = Some(password.to_string());
        self.user = Some(user);
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

/// A device holding a passkey of the account.
pub struct Device {
    pub authenticator: WebauthnAuthenticator<SoftPasskey>,
    pub username: String,
    pub credential_id: Value,
}

/// A passkey sign-in started from a signed-out browser, waiting for a device to answer it.
pub struct PendingSignIn {
    pub browser: Client,
    pub challenge: RequestChallengeResponse,
    pub nonce: String,
}

#[derive(Default)]
pub struct Passkeys {
    pub devices: Vec<Device>,
    pub pending: Option<PendingSignIn>,
    /// The browser that finished a passkey sign-in.
    pub signed_in_browser: Option<Client>,
    /// The passkey a visitor registered on another instance of the app.
    pub foreign: Option<ForeignPasskey>,
}

impl fmt::Debug for Passkeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} device(s), sign-in {}",
            self.devices.len(),
            if self.pending.is_some() {
                "pending"
            } else {
                "not started"
            }
        )
    }
}

/// What linking an authentication method through the app's services answered.
#[derive(Debug, Default)]
pub struct LinkedMethod {
    pub result: Option<Result<UserAuthMethod, String>>,
}

/// A passkey a visitor registered on another instance of the app, kept to be replayed.
pub struct ForeignPasskey {
    pub authenticator: WebauthnAuthenticator<SoftPasskey>,
    pub credential: RegisterPublicKeyCredential,
    pub nonce: String,
    pub username: String,
}
