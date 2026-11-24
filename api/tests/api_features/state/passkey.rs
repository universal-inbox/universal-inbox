use std::fmt;

use reqwest::Client;
use serde_json::Value;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
use webauthn_rs::prelude::{
    CreationChallengeResponse, RegisterPublicKeyCredential, RequestChallengeResponse,
};

pub struct Device {
    pub authenticator: WebauthnAuthenticator<SoftPasskey>,
    pub username: String,
    pub credential_id: Value,
}

pub struct PendingSignIn {
    pub browser: Client,
    pub challenge: RequestChallengeResponse,
    pub nonce: String,
}

#[derive(Default)]
pub struct Passkeys {
    pub devices: Vec<Device>,
    pub pending: Option<PendingSignIn>,
    pub signed_in_browser: Option<Client>,
    pub visitor: Option<VisitorPasskey>,
    pub reauth: Option<(RequestChallengeResponse, String)>,
    pub registration: Option<Registration>,
    pub registered_credential_id: Option<String>,
    pub started: Vec<PendingSignIn>,
}

pub struct Registration {
    pub browser: Client,
    pub authenticator: WebauthnAuthenticator<SoftPasskey>,
    pub challenge: CreationChallengeResponse,
    pub nonce: String,
    pub credential: Option<RegisterPublicKeyCredential>,
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

pub struct VisitorPasskey {
    pub authenticator: WebauthnAuthenticator<SoftPasskey>,
    pub credential: RegisterPublicKeyCredential,
    pub nonce: String,
    pub username: String,
}
