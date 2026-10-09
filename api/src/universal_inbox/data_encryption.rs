//! Maintenance of the data encrypted at rest: encrypting values written before encryption,
//! re-encrypting with the active key after a key rotation, and reporting which keys are used.
//! See `doc/src/config/data_encryption.md`.

use std::{collections::BTreeMap, fmt, sync::Arc};

use anyhow::anyhow;
use tracing::info;
use uuid::Uuid;

use crate::{
    repository::{
        Repository,
        data_encryption::{
            ENCRYPTED_COLUMNS, EncryptedColumn, EncryptedColumnFormat,
            GOOGLE_PROVIDER_USER_ID_TABLES,
        },
    },
    universal_inbox::UniversalInboxError,
    utils::crypto::{DataKeyring, KeyId, TokenKey, aad, decrypt_token, encrypt_token, token_key},
};

/// Label of the pre-envelope OAuth token format in [`ColumnStatus::values_per_key`]
pub const LEGACY_TOKEN_KEY_LABEL: &str = "legacy";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnStatus {
    pub column: String,
    /// Values not encrypted yet
    pub plaintext_values: i64,
    /// Encrypted values per key id (or [`LEGACY_TOKEN_KEY_LABEL`])
    pub values_per_key: BTreeMap<String, i64>,
    /// Encrypted values no configured key can open (token columns only: envelope columns
    /// are counted per key id without being decrypted)
    pub undecryptable_values: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataEncryptionStatus {
    pub columns: Vec<ColumnStatus>,
}

impl DataEncryptionStatus {
    pub fn plaintext_values(&self) -> i64 {
        self.columns.iter().map(|c| c.plaintext_values).sum()
    }

    /// Number of encrypted values per key id, all columns together
    pub fn values_per_key(&self) -> BTreeMap<String, i64> {
        let mut total = BTreeMap::new();
        for column in &self.columns {
            for (key, count) in &column.values_per_key {
                *total.entry(key.clone()).or_default() += count;
            }
        }
        total
    }

    /// Problems preventing to read every value with `keyring`
    pub fn unreadable_values(&self, keyring: &DataKeyring) -> Vec<String> {
        let mut problems = vec![];
        for column in &self.columns {
            for (key, count) in &column.values_per_key {
                let readable = match key.parse::<KeyId>() {
                    Ok(key_id) => keyring.has_key(key_id),
                    Err(_) => keyring.has_key(crate::utils::crypto::LEGACY_KEY_ID),
                };
                if !readable {
                    problems.push(format!(
                        "{}: {count} value(s) encrypted with key {key} which is not configured",
                        column.column
                    ));
                }
            }
            if column.undecryptable_values > 0 {
                problems.push(format!(
                    "{}: {} value(s) cannot be decrypted with any configured key",
                    column.column, column.undecryptable_values
                ));
            }
        }
        problems
    }
}

impl fmt::Display for DataEncryptionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for column in &self.columns {
            let per_key = column
                .values_per_key
                .iter()
                .map(|(key, count)| format!("key {key}: {count}"))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                f,
                "{}: plaintext: {}, {}{}",
                column.column,
                column.plaintext_values,
                if per_key.is_empty() {
                    "no encrypted value".to_string()
                } else {
                    per_key
                },
                if column.undecryptable_values > 0 {
                    format!(", undecryptable: {}", column.undecryptable_values)
                } else {
                    String::new()
                }
            )?;
        }
        Ok(())
    }
}

pub struct DataEncryptionService {
    repository: Arc<Repository>,
    keyring: Arc<DataKeyring>,
}

impl DataEncryptionService {
    pub fn new(repository: Arc<Repository>, keyring: Arc<DataKeyring>) -> Self {
        Self {
            repository,
            keyring,
        }
    }

    fn encrypt(
        &self,
        column: &EncryptedColumn,
        id: Uuid,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, UniversalInboxError> {
        match column.format {
            EncryptedColumnFormat::Envelope {
                aad_name,
                compressed: true,
            } => self
                .keyring
                .encrypt_compressed(plaintext, &aad(aad_name, id)),
            EncryptedColumnFormat::Envelope {
                aad_name,
                compressed: false,
            } => self.keyring.encrypt(plaintext, &aad(aad_name, id)),
            EncryptedColumnFormat::Token => {
                let plaintext = std::str::from_utf8(plaintext)
                    .map_err(|err| anyhow!("Token to encrypt is not valid UTF-8: {err}"))?;
                encrypt_token(plaintext, id.as_bytes(), &self.keyring)
            }
        }
    }

    fn decrypt(
        &self,
        column: &EncryptedColumn,
        id: Uuid,
        value: &[u8],
    ) -> Result<Vec<u8>, UniversalInboxError> {
        match column.format {
            EncryptedColumnFormat::Envelope { aad_name, .. } => {
                self.keyring.decrypt(value, &aad(aad_name, id))
            }
            EncryptedColumnFormat::Token => {
                Ok(decrypt_token(value, id.as_bytes(), &self.keyring)?.into_bytes())
            }
        }
    }

    pub async fn status(&self) -> Result<DataEncryptionStatus, UniversalInboxError> {
        let mut transaction = self.repository.begin().await?;
        let mut status = DataEncryptionStatus::default();
        for column in ENCRYPTED_COLUMNS {
            let mut column_status = ColumnStatus {
                column: column.name(),
                plaintext_values: self
                    .repository
                    .count_plaintext_values(&mut transaction, column)
                    .await?,
                ..Default::default()
            };
            match column.format {
                EncryptedColumnFormat::Envelope { .. } => {
                    for (key_id, count) in self
                        .repository
                        .count_values_per_key_id(&mut transaction, column)
                        .await?
                    {
                        column_status
                            .values_per_key
                            .insert(key_id.to_string(), count);
                    }
                }
                EncryptedColumnFormat::Token => {
                    let mut after = None;
                    loop {
                        let values = self
                            .repository
                            .fetch_encrypted_values(&mut transaction, column, after, None, 500)
                            .await?;
                        let Some((last_id, _)) = values.last() else {
                            break;
                        };
                        after = Some(*last_id);
                        for (id, value) in &values {
                            match token_key(value, id.as_bytes(), &self.keyring) {
                                Some(TokenKey::Envelope(key_id)) => {
                                    *column_status
                                        .values_per_key
                                        .entry(key_id.to_string())
                                        .or_default() += 1
                                }
                                Some(TokenKey::Legacy) => {
                                    *column_status
                                        .values_per_key
                                        .entry(LEGACY_TOKEN_KEY_LABEL.to_string())
                                        .or_default() += 1
                                }
                                None => column_status.undecryptable_values += 1,
                            }
                        }
                    }
                }
            }
            status.columns.push(column_status);
        }
        for table in GOOGLE_PROVIDER_USER_ID_TABLES {
            status.columns.push(ColumnStatus {
                column: format!("{table}.provider_user_id (Google email blind index)"),
                plaintext_values: self
                    .repository
                    .count_plaintext_google_provider_user_ids(&mut transaction, table)
                    .await?,
                ..Default::default()
            });
        }
        transaction.rollback().await.ok();
        Ok(status)
    }

    /// Encrypt the values written before their column was encrypted, then check every value
    /// can be read with the configured keys. Run before serving (container entrypoint).
    pub async fn encrypt_plaintext(
        &self,
        batch_size: i64,
    ) -> Result<DataEncryptionStatus, UniversalInboxError> {
        let mut lock_connection = self.repository.connect().await?;
        self.repository
            .acquire_data_encryption_lock(&mut lock_connection)
            .await?;
        let result = self.encrypt_plaintext_locked(batch_size).await;
        self.repository
            .release_data_encryption_lock(&mut lock_connection)
            .await?;
        let status = result?;

        let problems = status.unreadable_values(&self.keyring);
        if !problems.is_empty() {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Some encrypted data cannot be read with the configured data encryption keys \
                 (see doc/src/config/data_encryption.md):\n{}",
                problems.join("\n")
            )));
        }
        Ok(status)
    }

    async fn encrypt_plaintext_locked(
        &self,
        batch_size: i64,
    ) -> Result<DataEncryptionStatus, UniversalInboxError> {
        for column in ENCRYPTED_COLUMNS
            .iter()
            .filter(|column| column.plaintext_column.is_some())
        {
            let mut encrypted_count = 0;
            loop {
                let mut transaction = self.repository.begin().await?;
                let values = self
                    .repository
                    .fetch_plaintext_values(&mut transaction, column, batch_size)
                    .await?;
                if values.is_empty() {
                    break;
                }
                for (id, plaintext) in &values {
                    let encrypted = self.encrypt(column, *id, plaintext.as_bytes())?;
                    let blind_index = column
                        .blind_index_column
                        .map(|_| self.keyring.email_blind_index(plaintext))
                        .transpose()?;
                    self.repository
                        .store_encrypted_value(
                            &mut transaction,
                            column,
                            *id,
                            &encrypted,
                            blind_index.as_deref(),
                        )
                        .await?;
                }
                transaction
                    .commit()
                    .await
                    .map_err(|err| UniversalInboxError::DatabaseError {
                        source: err,
                        message: format!("Failed to commit encrypted {}", column.name()),
                    })?;
                encrypted_count += values.len();
                info!(
                    "{}: {encrypted_count} plaintext values encrypted",
                    column.name()
                );
            }
        }

        for table in GOOGLE_PROVIDER_USER_ID_TABLES {
            loop {
                let mut transaction = self.repository.begin().await?;
                let values = self
                    .repository
                    .fetch_plaintext_google_provider_user_ids(&mut transaction, table, batch_size)
                    .await?;
                if values.is_empty() {
                    break;
                }
                for (id, email) in &values {
                    let blind_index = self.keyring.email_blind_index(email)?;
                    self.repository
                        .store_provider_user_id(&mut transaction, table, *id, &blind_index)
                        .await?;
                }
                transaction
                    .commit()
                    .await
                    .map_err(|err| UniversalInboxError::DatabaseError {
                        source: err,
                        message: format!("Failed to commit {table} Google provider user ids"),
                    })?;
                info!(
                    "{table}: {} Google provider user ids replaced by their blind index",
                    values.len()
                );
            }
        }
        self.status().await
    }

    /// Re-encrypt with the active key every value sealed with another key (or in the legacy
    /// token format). Online and resumable: run it again after an interruption.
    pub async fn reencrypt(
        &self,
        batch_size: i64,
    ) -> Result<DataEncryptionStatus, UniversalInboxError> {
        let mut lock_connection = self.repository.connect().await?;
        self.repository
            .acquire_data_encryption_lock(&mut lock_connection)
            .await?;
        let result = self.reencrypt_locked(batch_size).await;
        self.repository
            .release_data_encryption_lock(&mut lock_connection)
            .await?;
        result
    }

    async fn reencrypt_locked(
        &self,
        batch_size: i64,
    ) -> Result<DataEncryptionStatus, UniversalInboxError> {
        let active_key_id = self.keyring.active_key_id();
        for column in ENCRYPTED_COLUMNS {
            let mut after = None;
            let mut reencrypted_count = 0;
            loop {
                let mut transaction = self.repository.begin().await?;
                let values = self
                    .repository
                    .fetch_encrypted_values(
                        &mut transaction,
                        column,
                        after,
                        Some(active_key_id),
                        batch_size,
                    )
                    .await?;
                let Some((last_id, _)) = values.last() else {
                    break;
                };
                after = Some(*last_id);
                for (id, value) in &values {
                    if column.format == EncryptedColumnFormat::Token
                        && token_key(value, id.as_bytes(), &self.keyring)
                            == Some(TokenKey::Envelope(active_key_id))
                    {
                        continue;
                    }
                    let plaintext = self.decrypt(column, *id, value)?;
                    let encrypted = self.encrypt(column, *id, &plaintext)?;
                    self.repository
                        .store_encrypted_value(&mut transaction, column, *id, &encrypted, None)
                        .await?;
                    reencrypted_count += 1;
                }
                transaction
                    .commit()
                    .await
                    .map_err(|err| UniversalInboxError::DatabaseError {
                        source: err,
                        message: format!("Failed to commit re-encrypted {}", column.name()),
                    })?;
            }
            info!(
                "{}: {reencrypted_count} values re-encrypted with key {active_key_id}",
                column.name()
            );
        }
        self.status().await
    }
}
