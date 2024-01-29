use anyhow::Context;
use base64::prelude::*;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey};
use ring::signature::KeyPair;
use ring::{rand::SystemRandom, signature::Ed25519KeyPair};
use serde::{Deserialize, Serialize};

use crate::universal_inbox::UniversalInboxError;

pub const JWT_SIGNING_ALGO: Algorithm = Algorithm::EdDSA;
pub const JWT_SESSION_KEY: &str = "jwt-session";

pub struct JWTSigningKeys {
    pub encoding_key: EncodingKey,
    pub decoding_key: DecodingKey,
}

pub struct JWTBase64EncodedSigningKeys {
    pub secret_key: String,
    pub public_key: String,
}

impl JWTBase64EncodedSigningKeys {
    pub fn generate() -> Result<Self, UniversalInboxError> {
        let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .context("Failed to generate JWT keys")?;
        let keypair = Ed25519KeyPair::from_pkcs8(doc.as_ref())
            .context("Failed to generate JWT deriving keys")?;
        let secret_key = BASE64_STANDARD.encode(doc.as_ref());
        let public_key = BASE64_STANDARD.encode(keypair.public_key().as_ref());
        Ok(JWTBase64EncodedSigningKeys {
            secret_key,
            public_key,
        })
    }
}

impl JWTSigningKeys {
    pub fn load_from_base64_encoded_keys(
        base64_encoded_keys: JWTBase64EncodedSigningKeys,
    ) -> Result<Self, UniversalInboxError> {
        let encoding_key = EncodingKey::from_ed_der(
            BASE64_STANDARD
                .decode(base64_encoded_keys.secret_key)
                .context("Failed to decode JWT secret key")?
                .as_ref(),
        );
        let decoding_key = DecodingKey::from_ed_der(
            BASE64_STANDARD
                .decode(base64_encoded_keys.public_key)
                .context("Failed to decode JWT public key")?
                .as_ref(),
        );
        Ok(JWTSigningKeys {
            encoding_key,
            decoding_key,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub exp: usize,
    pub iat: usize,
    pub sub: String,
    pub jti: String,
}
