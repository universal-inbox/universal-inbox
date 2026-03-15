use anyhow::Context;
use ring::aead::{self, Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey};
use ring::error::Unspecified;
use ring::rand::{SecureRandom, SystemRandom};
use secrecy::CloneableSecret;
use secrecy::zeroize::Zeroize;

use crate::universal_inbox::UniversalInboxError;

const NONCE_LEN: usize = 12; // 96-bit nonce for AES-256-GCM

struct SingleNonce(Option<[u8; NONCE_LEN]>);

impl NonceSequence for SingleNonce {
    fn advance(&mut self) -> Result<Nonce, Unspecified> {
        let nonce_bytes = self.0.take().ok_or(Unspecified)?;
        Ok(Nonce::assume_unique_for_key(nonce_bytes))
    }
}

/// Encryption key for OAuth tokens, wrapping a 32-byte AES-256 key.
#[derive(Clone)]
pub struct TokenEncryptionKey {
    key_bytes: Vec<u8>,
}

impl Zeroize for TokenEncryptionKey {
    fn zeroize(&mut self) {
        self.key_bytes.zeroize();
    }
}

impl CloneableSecret for TokenEncryptionKey {}

impl TokenEncryptionKey {
    pub fn from_hex(hex_str: &str) -> Result<Self, UniversalInboxError> {
        let key_bytes =
            hex::decode(hex_str).context("Failed to decode token encryption key from hex")?;
        if key_bytes.len() != 32 {
            return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Token encryption key must be 32 bytes (64 hex chars), got {} bytes",
                key_bytes.len()
            )));
        }
        Ok(Self { key_bytes })
    }

    fn unbound_key(&self) -> Result<UnboundKey, UniversalInboxError> {
        UnboundKey::new(&aead::AES_256_GCM, &self.key_bytes).map_err(|_| {
            UniversalInboxError::Unexpected(anyhow::anyhow!("Failed to create AES-256-GCM key"))
        })
    }
}

/// Encrypt a plaintext token. Returns nonce (12 bytes) prepended to ciphertext+tag.
/// `aad_context` binds the ciphertext to a specific context (e.g. connection ID bytes)
/// so that it cannot be decrypted in a different context.
pub fn encrypt_token(
    plaintext: &str,
    aad_context: &[u8],
    key: &TokenEncryptionKey,
) -> Result<Vec<u8>, UniversalInboxError> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes).map_err(|_| {
        UniversalInboxError::Unexpected(anyhow::anyhow!("Failed to generate nonce"))
    })?;

    let unbound_key = key.unbound_key()?;
    let mut sealing_key = SealingKey::new(unbound_key, SingleNonce(Some(nonce_bytes)));

    let mut in_out = plaintext.as_bytes().to_vec();
    sealing_key
        .seal_in_place_append_tag(Aad::from(aad_context), &mut in_out)
        .map_err(|_| UniversalInboxError::Unexpected(anyhow::anyhow!("Failed to encrypt token")))?;

    let mut result = Vec::with_capacity(NONCE_LEN + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);
    Ok(result)
}

/// Decrypt a token from nonce (12 bytes) + ciphertext+tag format.
/// `aad_context` must match the value used during encryption.
pub fn decrypt_token(
    ciphertext: &[u8],
    aad_context: &[u8],
    key: &TokenEncryptionKey,
) -> Result<String, UniversalInboxError> {
    if ciphertext.len() < NONCE_LEN + aead::AES_256_GCM.tag_len() {
        return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
            "Ciphertext too short to contain nonce and tag"
        )));
    }

    let (nonce_bytes, encrypted) = ciphertext.split_at(NONCE_LEN);
    let mut nonce_arr = [0u8; NONCE_LEN];
    nonce_arr.copy_from_slice(nonce_bytes);

    let unbound_key = key.unbound_key()?;
    let mut opening_key = OpeningKey::new(unbound_key, SingleNonce(Some(nonce_arr)));

    let mut in_out = encrypted.to_vec();
    let decrypted = opening_key
        .open_in_place(Aad::from(aad_context), &mut in_out)
        .map_err(|_| {
            UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Failed to decrypt token: invalid key or corrupted ciphertext"
            ))
        })?;

    String::from_utf8(decrypted.to_vec()).map_err(|err| {
        UniversalInboxError::Unexpected(anyhow::anyhow!(
            "Decrypted token is not valid UTF-8: {err}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn test_key() -> TokenEncryptionKey {
        // 32-byte key as 64 hex chars
        TokenEncryptionKey::from_hex(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap()
    }

    const TEST_AAD: &[u8] = b"test-connection-id";

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = "xoxb-test-access-token-12345";

        let encrypted = encrypt_token(plaintext, TEST_AAD, &key).unwrap();
        let decrypted = decrypt_token(&encrypted, TEST_AAD, &key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let key = test_key();
        let plaintext = "same-token";

        let encrypted1 = encrypt_token(plaintext, TEST_AAD, &key).unwrap();
        let encrypted2 = encrypt_token(plaintext, TEST_AAD, &key).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same value
        assert_eq!(
            decrypt_token(&encrypted1, TEST_AAD, &key).unwrap(),
            plaintext
        );
        assert_eq!(
            decrypt_token(&encrypted2, TEST_AAD, &key).unwrap(),
            plaintext
        );
    }

    #[test]
    fn test_nonce_uniqueness() {
        let key = test_key();
        let mut nonces = HashSet::new();

        for _ in 0..100 {
            let encrypted = encrypt_token("token", TEST_AAD, &key).unwrap();
            let nonce = &encrypted[..NONCE_LEN];
            nonces.insert(nonce.to_vec());
        }

        assert_eq!(nonces.len(), 100, "All nonces should be unique");
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key1 = test_key();
        let key2 = TokenEncryptionKey::from_hex(
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        )
        .unwrap();

        let encrypted = encrypt_token("secret-token", TEST_AAD, &key1).unwrap();
        assert!(decrypt_token(&encrypted, TEST_AAD, &key2).is_err());
    }

    #[test]
    fn test_decrypt_with_wrong_aad_fails() {
        let key = test_key();
        let encrypted = encrypt_token("secret-token", b"connection-1", &key).unwrap();
        assert!(decrypt_token(&encrypted, b"connection-2", &key).is_err());
    }

    #[test]
    fn test_decrypt_corrupted_ciphertext_fails() {
        let key = test_key();
        let mut encrypted = encrypt_token("token", TEST_AAD, &key).unwrap();

        // Corrupt a byte in the ciphertext
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;

        assert!(decrypt_token(&encrypted, TEST_AAD, &key).is_err());
    }

    #[test]
    fn test_decrypt_too_short_fails() {
        let key = test_key();
        assert!(decrypt_token(&[0u8; 10], TEST_AAD, &key).is_err());
    }

    #[test]
    fn test_key_from_hex_wrong_length_fails() {
        assert!(TokenEncryptionKey::from_hex("0123456789abcdef").is_err());
    }

    #[test]
    fn test_key_from_hex_invalid_hex_fails() {
        assert!(
            TokenEncryptionKey::from_hex(
                "not-hex-at-all-not-hex-at-all-not-hex-at-all-not-hex-at-all-1234"
            )
            .is_err()
        );
    }

    #[test]
    fn test_encrypt_decrypt_empty_string() {
        let key = test_key();
        let encrypted = encrypt_token("", TEST_AAD, &key).unwrap();
        let decrypted = decrypt_token(&encrypted, TEST_AAD, &key).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_encrypt_decrypt_unicode() {
        let key = test_key();
        let plaintext = "token-with-émojis-🔑";
        let encrypted = encrypt_token(plaintext, TEST_AAD, &key).unwrap();
        let decrypted = decrypt_token(&encrypted, TEST_AAD, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
