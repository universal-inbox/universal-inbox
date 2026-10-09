//! Field-level encryption of data stored at rest.
//!
//! Values are sealed with AES-256-GCM into a versioned envelope:
//!
//! ```text
//! version (1) | key_id (1) | flags (1) | nonce (12) | ciphertext | tag (16)
//! ```
//!
//! - `key_id` names the key of the [`DataKeyring`] that sealed the value, so keys can be
//!   rotated: new values use the active key while older keys stay readable until every
//!   value has been re-encrypted (see `doc/src/config/data_encryption.md`).
//! - `flags` bit 0 means the plaintext was zstd-compressed before sealing (large JSON
//!   payloads: ciphertext defeats Postgres TOAST compression).
//! - The additional authenticated data (AAD) binds a ciphertext to its table, column and
//!   row (see [`aad`]) so it cannot be moved to another row or column. The header is
//!   authenticated too: it is prepended to the AAD.
//!
//! OAuth tokens written before the envelope existed are stored as `nonce || ciphertext || tag`
//! sealed with key [`LEGACY_KEY_ID`]. [`decrypt_token`] still reads them.

use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, OnceLock},
};

use anyhow::{Context, anyhow};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    hmac,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::universal_inbox::UniversalInboxError;

pub type KeyId = u8;

/// Key id used by ciphertexts written before the envelope format existed.
pub const LEGACY_KEY_ID: KeyId = 1;

const ENVELOPE_VERSION: u8 = 1;
const HEADER_LEN: usize = 3;
const NONCE_LEN: usize = 12; // 96-bit nonce for AES-256-GCM
const TAG_LEN: usize = 16;
const FLAG_ZSTD: u8 = 0b0000_0001;
const ZSTD_LEVEL: i32 = 3;

/// Build the AAD binding a value to `<table>.<column>` of the row `row_id`.
pub fn aad(table_column: &str, row_id: Uuid) -> Vec<u8> {
    let mut aad = Vec::with_capacity(table_column.len() + 16);
    aad.extend_from_slice(table_column.as_bytes());
    aad.extend_from_slice(row_id.as_bytes());
    aad
}

/// Set of AES-256 keys indexed by key id, one of them being active for new encryptions.
pub struct DataKeyring {
    active_key_id: KeyId,
    keys: BTreeMap<KeyId, LessSafeKey>,
    /// Key of the blind indexes (HMAC-SHA256), see [`DataKeyring::blind_index`]
    blind_index_key: Option<hmac::Key>,
    rng: SystemRandom,
}

impl fmt::Debug for DataKeyring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataKeyring")
            .field("active_key_id", &self.active_key_id)
            .field("key_ids", &self.key_ids())
            .finish()
    }
}

impl DataKeyring {
    /// Build a keyring from hex-encoded 32-byte keys.
    pub fn new<'a>(
        active_key_id: KeyId,
        hex_keys: impl IntoIterator<Item = (KeyId, &'a str)>,
    ) -> Result<Self, UniversalInboxError> {
        let mut keys = BTreeMap::new();
        for (key_id, hex_key) in hex_keys {
            let mut key_bytes = hex::decode(hex_key.trim())
                .with_context(|| format!("Data encryption key {key_id} is not valid hex"))?;
            if key_bytes.len() != 32 {
                return Err(UniversalInboxError::Unexpected(anyhow!(
                    "Data encryption key {key_id} must be 32 bytes (64 hex chars, `openssl rand -hex 32`), got {} bytes",
                    key_bytes.len()
                )));
            }
            let unbound_key = UnboundKey::new(&aead::AES_256_GCM, &key_bytes)
                .map_err(|_| anyhow!("Failed to create AES-256-GCM key {key_id}"));
            secrecy::zeroize::Zeroize::zeroize(&mut key_bytes);
            keys.insert(key_id, LessSafeKey::new(unbound_key?));
        }

        if !keys.contains_key(&active_key_id) {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Active data encryption key {active_key_id} is not configured"
            )));
        }

        Ok(Self {
            active_key_id,
            keys,
            blind_index_key: None,
            rng: SystemRandom::new(),
        })
    }

    /// Set the hex-encoded 32-byte key of the blind indexes. It is independent from the
    /// encryption keys: rotating them must not change the blind indexes.
    pub fn with_blind_index_key(mut self, hex_key: &str) -> Result<Self, UniversalInboxError> {
        let mut key_bytes =
            hex::decode(hex_key.trim()).context("Blind index key is not valid hex")?;
        if key_bytes.len() != 32 {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Blind index key must be 32 bytes (64 hex chars, `openssl rand -hex 32`), got {} bytes",
                key_bytes.len()
            )));
        }
        self.blind_index_key = Some(hmac::Key::new(hmac::HMAC_SHA256, &key_bytes));
        secrecy::zeroize::Zeroize::zeroize(&mut key_bytes);
        Ok(self)
    }

    /// Keyed, deterministic digest (hex HMAC-SHA256) of `value`, stored next to an encrypted
    /// value to look it up by equality or enforce its uniqueness without decrypting it.
    pub fn blind_index(&self, value: &str) -> Result<String, UniversalInboxError> {
        let key = self.blind_index_key.as_ref().ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!("Blind index key is not configured"))
        })?;
        Ok(hex::encode(hmac::sign(key, value.as_bytes()).as_ref()))
    }

    /// Blind index of an email address, case-insensitive
    pub fn email_blind_index(&self, email: &str) -> Result<String, UniversalInboxError> {
        self.blind_index(&normalize_email(email))
    }

    pub fn active_key_id(&self) -> KeyId {
        self.active_key_id
    }

    pub fn key_ids(&self) -> Vec<KeyId> {
        self.keys.keys().copied().collect()
    }

    pub fn has_key(&self, key_id: KeyId) -> bool {
        self.keys.contains_key(&key_id)
    }

    /// Seal `plaintext` with the active key.
    pub fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, UniversalInboxError> {
        self.seal(plaintext, aad, 0)
    }

    /// Compress `plaintext` with zstd, then seal it with the active key.
    pub fn encrypt_compressed(
        &self,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, UniversalInboxError> {
        let compressed = zstd::encode_all(plaintext, ZSTD_LEVEL)
            .context("Failed to compress data before encryption")?;
        self.seal(&compressed, aad, FLAG_ZSTD)
    }

    /// Serialize `value` to JSON, compress and seal it.
    pub fn encrypt_json<T: Serialize>(
        &self,
        value: &T,
        aad: &[u8],
    ) -> Result<Vec<u8>, UniversalInboxError> {
        let json = serde_json::to_vec(value).context("Failed to serialize data to encrypt")?;
        self.encrypt_compressed(&json, aad)
    }

    /// Open an envelope sealed by [`encrypt`](Self::encrypt) or
    /// [`encrypt_compressed`](Self::encrypt_compressed).
    pub fn decrypt(&self, envelope: &[u8], aad: &[u8]) -> Result<Vec<u8>, UniversalInboxError> {
        let header = EnvelopeHeader::parse(envelope).ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!("Encrypted value has an invalid envelope"))
        })?;
        let key = self.keys.get(&header.key_id).ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Encrypted value uses data encryption key {} which is not configured",
                header.key_id
            ))
        })?;

        let (nonce_bytes, sealed) = envelope[HEADER_LEN..].split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| anyhow!("Encrypted value has an invalid nonce"))?;
        let mut in_out = sealed.to_vec();
        let opened = key
            .open_in_place(
                nonce,
                Aad::from(header_aad(&envelope[..HEADER_LEN], aad)),
                &mut in_out,
            )
            .map_err(|_| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Failed to decrypt value: invalid key or corrupted ciphertext"
                ))
            })?;

        if header.flags & FLAG_ZSTD != 0 {
            Ok(zstd::decode_all(&opened[..]).context("Failed to decompress decrypted data")?)
        } else {
            Ok(opened.to_vec())
        }
    }

    /// Open an envelope written by [`encrypt_json`](Self::encrypt_json).
    pub fn decrypt_json<T: DeserializeOwned>(
        &self,
        envelope: &[u8],
        aad: &[u8],
    ) -> Result<T, UniversalInboxError> {
        let json = self.decrypt(envelope, aad)?;
        Ok(serde_json::from_slice(&json).context("Failed to deserialize decrypted data")?)
    }

    /// Open an envelope holding UTF-8 text.
    pub fn decrypt_string(
        &self,
        envelope: &[u8],
        aad: &[u8],
    ) -> Result<String, UniversalInboxError> {
        String::from_utf8(self.decrypt(envelope, aad)?).map_err(|err| {
            UniversalInboxError::Unexpected(anyhow!("Decrypted value is not valid UTF-8: {err}"))
        })
    }

    fn seal(
        &self,
        plaintext: &[u8],
        aad: &[u8],
        flags: u8,
    ) -> Result<Vec<u8>, UniversalInboxError> {
        let key = self
            .keys
            .get(&self.active_key_id)
            .expect("active key presence is checked at construction");
        let mut nonce_bytes = [0u8; NONCE_LEN];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|_| anyhow!("Failed to generate nonce"))?;

        let header = [ENVELOPE_VERSION, self.active_key_id, flags];
        let mut envelope = Vec::with_capacity(HEADER_LEN + NONCE_LEN + plaintext.len() + TAG_LEN);
        envelope.extend_from_slice(&header);
        envelope.extend_from_slice(&nonce_bytes);
        let mut in_out = plaintext.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(header_aad(&header, aad)),
            &mut in_out,
        )
        .map_err(|_| anyhow!("Failed to encrypt value"))?;
        envelope.extend_from_slice(&in_out);
        Ok(envelope)
    }

    fn decrypt_legacy(
        &self,
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, UniversalInboxError> {
        if ciphertext.len() < NONCE_LEN + TAG_LEN {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Ciphertext too short to contain nonce and tag"
            )));
        }
        let key = self.keys.get(&LEGACY_KEY_ID).ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Legacy encrypted value needs data encryption key {LEGACY_KEY_ID} which is not configured"
            ))
        })?;
        let (nonce_bytes, sealed) = ciphertext.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| anyhow!("Encrypted value has an invalid nonce"))?;
        let mut in_out = sealed.to_vec();
        let opened = key
            .open_in_place(nonce, Aad::from(aad), &mut in_out)
            .map_err(|_| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Failed to decrypt token: invalid key or corrupted ciphertext"
                ))
            })?;
        Ok(opened.to_vec())
    }
}

/// Normalize an email address before computing its blind index: email lookups are
/// case-insensitive.
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn header_aad(header: &[u8], aad: &[u8]) -> Vec<u8> {
    [header, aad].concat()
}

struct EnvelopeHeader {
    key_id: KeyId,
    flags: u8,
}

impl EnvelopeHeader {
    fn parse(envelope: &[u8]) -> Option<Self> {
        if envelope.len() < HEADER_LEN + NONCE_LEN + TAG_LEN || envelope[0] != ENVELOPE_VERSION {
            return None;
        }
        Some(Self {
            key_id: envelope[1],
            flags: envelope[2],
        })
    }
}

/// Key that sealed a stored token, as found by [`token_key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKey {
    Envelope(KeyId),
    /// Pre-envelope format, sealed with [`LEGACY_KEY_ID`]
    Legacy,
}

/// Encrypt an OAuth token with the active key.
/// `aad_context` binds the ciphertext to a specific context (e.g. connection ID bytes)
/// so that it cannot be decrypted in a different context.
pub fn encrypt_token(
    plaintext: &str,
    aad_context: &[u8],
    keyring: &DataKeyring,
) -> Result<Vec<u8>, UniversalInboxError> {
    keyring.encrypt(plaintext.as_bytes(), aad_context)
}

/// Decrypt an OAuth token stored either as an envelope or in the legacy format.
/// `aad_context` must match the value used during encryption.
pub fn decrypt_token(
    ciphertext: &[u8],
    aad_context: &[u8],
    keyring: &DataKeyring,
) -> Result<String, UniversalInboxError> {
    let plaintext = match token_key(ciphertext, aad_context, keyring) {
        Some(TokenKey::Envelope(_)) => keyring.decrypt(ciphertext, aad_context)?,
        _ => keyring.decrypt_legacy(ciphertext, aad_context)?,
    };
    String::from_utf8(plaintext).map_err(|err| {
        UniversalInboxError::Unexpected(anyhow!("Decrypted token is not valid UTF-8: {err}"))
    })
}

/// Find out which key sealed a stored token. A legacy ciphertext starts with a random nonce
/// that may look like an envelope header: the AEAD tag check tells both formats apart.
/// Returns `None` when no configured key opens the token.
pub fn token_key(ciphertext: &[u8], aad_context: &[u8], keyring: &DataKeyring) -> Option<TokenKey> {
    if let Some(header) = EnvelopeHeader::parse(ciphertext)
        && keyring.decrypt(ciphertext, aad_context).is_ok()
    {
        return Some(TokenKey::Envelope(header.key_id));
    }
    keyring
        .decrypt_legacy(ciphertext, aad_context)
        .ok()
        .map(|_| TokenKey::Legacy)
}

/// Key id of an envelope, without decrypting it.
pub fn envelope_key_id(envelope: &[u8]) -> Option<KeyId> {
    EnvelopeHeader::parse(envelope).map(|header| header.key_id)
}

static DATA_KEYRING: OnceLock<Arc<DataKeyring>> = OnceLock::new();

/// Install the process-wide keyring used to decrypt rows while they are decoded.
///
/// Row decoding goes through `sqlx::FromRow`, which carries no context, hence a process-wide
/// keyring rather than one passed down to every conversion. The first installed keyring wins:
/// a process only ever runs with the keys of its configuration.
pub fn install_data_keyring(keyring: Arc<DataKeyring>) -> Arc<DataKeyring> {
    DATA_KEYRING.get_or_init(|| keyring).clone()
}

/// The keyring installed by [`install_data_keyring`].
pub fn data_keyring() -> Result<&'static Arc<DataKeyring>, UniversalInboxError> {
    DATA_KEYRING.get().ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow!("Data encryption keyring is not installed"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use ring::aead::{BoundKey, NonceSequence, SealingKey};
    use ring::error::Unspecified;

    const KEY_1: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const KEY_2: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    const TEST_AAD: &[u8] = b"test-connection-id";

    fn keyring(active: KeyId, keys: &[(KeyId, &'static str)]) -> DataKeyring {
        DataKeyring::new(active, keys.iter().copied()).unwrap()
    }

    fn test_keyring() -> DataKeyring {
        keyring(1, &[(1, KEY_1)])
    }

    struct SingleNonce(Option<[u8; NONCE_LEN]>);

    impl NonceSequence for SingleNonce {
        fn advance(&mut self) -> Result<Nonce, Unspecified> {
            Ok(Nonce::assume_unique_for_key(
                self.0.take().ok_or(Unspecified)?,
            ))
        }
    }

    /// The pre-envelope OAuth token format: nonce || ciphertext || tag
    fn legacy_encrypt(
        plaintext: &str,
        aad: &[u8],
        hex_key: &str,
        nonce: [u8; NONCE_LEN],
    ) -> Vec<u8> {
        let key_bytes = hex::decode(hex_key).unwrap();
        let mut sealing_key = SealingKey::new(
            UnboundKey::new(&aead::AES_256_GCM, &key_bytes).unwrap(),
            SingleNonce(Some(nonce)),
        );
        let mut in_out = plaintext.as_bytes().to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::from(aad), &mut in_out)
            .unwrap();
        [nonce.to_vec(), in_out].concat()
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let keyring = test_keyring();
        let plaintext = "xoxb-test-access-token-12345";

        let encrypted = encrypt_token(plaintext, TEST_AAD, &keyring).unwrap();
        let decrypted = decrypt_token(&encrypted, TEST_AAD, &keyring).unwrap();

        assert_eq!(decrypted, plaintext);
        assert_eq!(envelope_key_id(&encrypted), Some(1));
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let keyring = test_keyring();
        let plaintext = "same-token";

        let encrypted1 = encrypt_token(plaintext, TEST_AAD, &keyring).unwrap();
        let encrypted2 = encrypt_token(plaintext, TEST_AAD, &keyring).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same value
        assert_eq!(
            decrypt_token(&encrypted1, TEST_AAD, &keyring).unwrap(),
            plaintext
        );
        assert_eq!(
            decrypt_token(&encrypted2, TEST_AAD, &keyring).unwrap(),
            plaintext
        );
    }

    #[test]
    fn test_nonce_uniqueness() {
        let keyring = test_keyring();
        let mut nonces = HashSet::new();

        for _ in 0..100 {
            let encrypted = encrypt_token("token", TEST_AAD, &keyring).unwrap();
            nonces.insert(encrypted[HEADER_LEN..HEADER_LEN + NONCE_LEN].to_vec());
        }

        assert_eq!(nonces.len(), 100, "All nonces should be unique");
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let encrypted =
            encrypt_token("secret-token", TEST_AAD, &keyring(1, &[(1, KEY_1)])).unwrap();
        assert!(decrypt_token(&encrypted, TEST_AAD, &keyring(1, &[(1, KEY_2)])).is_err());
    }

    #[test]
    fn test_decrypt_with_unknown_key_id_fails() {
        let encrypted = keyring(2, &[(1, KEY_1), (2, KEY_2)])
            .encrypt(b"value", TEST_AAD)
            .unwrap();
        let error = test_keyring().decrypt(&encrypted, TEST_AAD).unwrap_err();
        assert!(format!("{error:?}").contains("key 2 which is not configured"));
    }

    #[test]
    fn test_decrypt_with_wrong_aad_fails() {
        let keyring = test_keyring();
        let encrypted = encrypt_token("secret-token", b"connection-1", &keyring).unwrap();
        assert!(decrypt_token(&encrypted, b"connection-2", &keyring).is_err());

        let id = Uuid::new_v4();
        let encrypted = keyring.encrypt(b"body", &aad("task.body", id)).unwrap();
        assert!(
            keyring
                .decrypt(&encrypted, &aad("task.body", Uuid::new_v4()))
                .is_err()
        );
        assert!(keyring.decrypt(&encrypted, &aad("task.title", id)).is_err());
        assert_eq!(
            keyring.decrypt(&encrypted, &aad("task.body", id)).unwrap(),
            b"body"
        );
    }

    #[test]
    fn test_decrypt_corrupted_ciphertext_fails() {
        let keyring = test_keyring();
        let mut encrypted = encrypt_token("token", TEST_AAD, &keyring).unwrap();

        // Corrupt a byte in the ciphertext
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;

        assert!(decrypt_token(&encrypted, TEST_AAD, &keyring).is_err());
    }

    #[test]
    fn test_tampered_header_fails() {
        let keyring = keyring(1, &[(1, KEY_1), (2, KEY_1)]);
        let encrypted = keyring.encrypt_compressed(b"value", TEST_AAD).unwrap();

        let mut flags_cleared = encrypted.clone();
        flags_cleared[2] = 0;
        assert!(keyring.decrypt(&flags_cleared, TEST_AAD).is_err());

        // Same key bytes under another id: the key id is authenticated as well
        let mut key_id_changed = encrypted;
        key_id_changed[1] = 2;
        assert!(keyring.decrypt(&key_id_changed, TEST_AAD).is_err());
    }

    #[test]
    fn test_decrypt_too_short_fails() {
        let keyring = test_keyring();
        assert!(decrypt_token(&[0u8; 10], TEST_AAD, &keyring).is_err());
        assert!(keyring.decrypt(&[1u8; 10], TEST_AAD).is_err());
    }

    #[test]
    fn test_key_from_hex_wrong_length_fails() {
        assert!(DataKeyring::new(1, [(1, "0123456789abcdef")]).is_err());
    }

    #[test]
    fn test_key_from_hex_invalid_hex_fails() {
        assert!(
            DataKeyring::new(
                1,
                [(
                    1,
                    "not-hex-at-all-not-hex-at-all-not-hex-at-all-not-hex-at-all-1234"
                )]
            )
            .is_err()
        );
    }

    #[test]
    fn test_missing_active_key_fails() {
        assert!(DataKeyring::new(2, [(1, KEY_1)]).is_err());
    }

    #[test]
    fn test_encrypt_decrypt_empty_string() {
        let keyring = test_keyring();
        let encrypted = encrypt_token("", TEST_AAD, &keyring).unwrap();
        assert_eq!(decrypt_token(&encrypted, TEST_AAD, &keyring).unwrap(), "");
    }

    #[test]
    fn test_encrypt_decrypt_unicode() {
        let keyring = test_keyring();
        let plaintext = "token-with-émojis-🔑";
        let encrypted = encrypt_token(plaintext, TEST_AAD, &keyring).unwrap();
        assert_eq!(
            decrypt_token(&encrypted, TEST_AAD, &keyring).unwrap(),
            plaintext
        );
    }

    #[test]
    fn test_compressed_json_roundtrip() {
        let keyring = test_keyring();
        let value = serde_json::json!({ "type": "SlackThread", "text": "hello ".repeat(1000) });

        let encrypted = keyring.encrypt_json(&value, TEST_AAD).unwrap();

        assert_eq!(encrypted[2] & FLAG_ZSTD, FLAG_ZSTD);
        assert!(
            encrypted.len() < 500,
            "repetitive JSON should be compressed"
        );
        let decrypted: serde_json::Value = keyring.decrypt_json(&encrypted, TEST_AAD).unwrap();
        assert_eq!(decrypted, value);
    }

    #[test]
    fn test_rotation_keeps_old_values_readable() {
        let old_value = keyring(1, &[(1, KEY_1)]).encrypt(b"old", TEST_AAD).unwrap();
        let rotated = keyring(2, &[(1, KEY_1), (2, KEY_2)]);

        let new_value = rotated.encrypt(b"new", TEST_AAD).unwrap();

        assert_eq!(envelope_key_id(&new_value), Some(2));
        assert_eq!(rotated.decrypt(&old_value, TEST_AAD).unwrap(), b"old");
        assert_eq!(rotated.decrypt(&new_value, TEST_AAD).unwrap(), b"new");
    }

    #[test]
    fn test_email_blind_index() {
        let keyring = test_keyring().with_blind_index_key(KEY_2).unwrap();
        let other_keyring = test_keyring().with_blind_index_key(KEY_1).unwrap();

        let index = keyring.email_blind_index("John.Doe@Example.com").unwrap();

        assert_eq!(index.len(), 64);
        assert_eq!(
            keyring.email_blind_index(" john.doe@example.com ").unwrap(),
            index
        );
        assert_ne!(
            keyring.email_blind_index("jane.doe@example.com").unwrap(),
            index
        );
        assert_ne!(
            other_keyring
                .email_blind_index("john.doe@example.com")
                .unwrap(),
            index
        );
        assert!(
            test_keyring()
                .email_blind_index("john.doe@example.com")
                .is_err()
        );
        assert!(test_keyring().with_blind_index_key("1234").is_err());
    }

    #[test]
    fn test_legacy_token_still_decrypts() {
        let keyring = test_keyring();
        // Nonces starting like an envelope header must still be read as legacy tokens
        for nonce in [[7u8; NONCE_LEN], [1u8; NONCE_LEN]] {
            let legacy = legacy_encrypt("legacy-token", TEST_AAD, KEY_1, nonce);

            assert_eq!(
                decrypt_token(&legacy, TEST_AAD, &keyring).unwrap(),
                "legacy-token"
            );
            assert_eq!(
                token_key(&legacy, TEST_AAD, &keyring),
                Some(TokenKey::Legacy)
            );
        }
    }

    #[test]
    fn test_token_key_detects_envelope_key() {
        let rotated = keyring(2, &[(1, KEY_1), (2, KEY_2)]);
        let encrypted = encrypt_token("token", TEST_AAD, &rotated).unwrap();

        assert_eq!(
            token_key(&encrypted, TEST_AAD, &rotated),
            Some(TokenKey::Envelope(2))
        );
        assert_eq!(token_key(&encrypted, b"other", &rotated), None);
    }
}
