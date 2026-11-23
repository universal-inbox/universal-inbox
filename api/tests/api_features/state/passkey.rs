//! What the scenarios of the passkey features keep between their steps.

use std::fmt;

use reqwest::Client;
use serde_json::Value;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
use webauthn_rs::prelude::{RegisterPublicKeyCredential, RequestChallengeResponse};

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
    /// The passkey a visitor registered, on this app or on another instance of it.
    pub visitor: Option<VisitorPasskey>,
    /// A passkey re-authentication that was started and waits for a device to answer it.
    pub reauth: Option<(RequestChallengeResponse, String)>,
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

/// A passkey a visitor registered on another instance of the app, kept to be replayed.
pub struct VisitorPasskey {
    pub authenticator: WebauthnAuthenticator<SoftPasskey>,
    pub credential: RegisterPublicKeyCredential,
    pub nonce: String,
    pub username: String,
}
