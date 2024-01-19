use anyhow::Context;
use base64::prelude::*;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header};
use ring::signature::KeyPair;
use ring::{rand::SystemRandom, signature::Ed25519KeyPair};
use serde::{Deserialize, Serialize};

use crate::universal_inbox::UniversalInboxError;

pub const JWT_SIGNING_ALGO: Algorithm = Algorithm::EdDSA;

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
}

impl Claims {
    pub fn new_jwt_token(
        sub: String,
        ttl: &JWTttl,
        encoding_key: &EncodingKey,
    ) -> Result<String, UniversalInboxError> {
        let claims = Claims {
            iat: Utc::now().timestamp() as usize,
            exp: (Utc::now() + Duration::days(ttl.0)).timestamp() as usize,
            sub,
        };
        Ok(
            jsonwebtoken::encode(&Header::new(JWT_SIGNING_ALGO), &claims, encoding_key)
                .context("Failed to encode JSON web token")?,
        )
    }
}

pub struct JWTttl(pub i64);
