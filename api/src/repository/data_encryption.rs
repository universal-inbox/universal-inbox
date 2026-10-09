//! Generic access to the encrypted columns, for the data encryption maintenance commands
//! (encrypting plaintext rows, key rotation, status). Queries are built dynamically
//! (`AssertSqlSafe`) from the table and column names of [`ENCRYPTED_COLUMNS`] only, never
//! from user input; values are always bound.

use sqlx::{AssertSqlSafe, PgConnection, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{repository::Repository, universal_inbox::UniversalInboxError, utils::crypto::KeyId};

/// How the values of an encrypted column are sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptedColumnFormat {
    /// Versioned envelope with `aad(aad_name, row_id)` as AAD, the plaintext being compressed
    /// when `compressed` is set
    Envelope {
        aad_name: &'static str,
        compressed: bool,
    },
    /// OAuth token, with the row id bytes as AAD; may still use the pre-envelope format
    Token,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncryptedColumn {
    pub table: &'static str,
    pub id_column: &'static str,
    pub column: &'static str,
    /// Column still holding values written before encryption, if any. They are moved into
    /// `column` by `data-encryption encrypt-plaintext`.
    pub plaintext_column: Option<&'static str>,
    /// Column holding the email blind index of the value (case-insensitive lookups and
    /// uniqueness), filled along with `column` by `data-encryption encrypt-plaintext`
    pub blind_index_column: Option<&'static str>,
    pub format: EncryptedColumnFormat,
}

impl EncryptedColumn {
    pub fn name(&self) -> String {
        format!("{}.{}", self.table.trim_matches('"'), self.column)
    }
}

/// Tables whose `provider_user_id` holds the email of a Google account, stored as its email
/// blind index
pub const GOOGLE_PROVIDER_USER_ID_TABLES: &[&str] =
    &["integration_connection", "oauth_grant_revocation"];

/// Google providers, whose `provider_user_id` is the account email
const GOOGLE_PROVIDER_KINDS: &str = "('GoogleMail', 'GoogleDrive', 'GoogleCalendar')";

/// Every column encrypted at rest.
pub const ENCRYPTED_COLUMNS: &[EncryptedColumn] = &[
    EncryptedColumn {
        table: "\"user\"",
        id_column: "id",
        column: "email_enc",
        plaintext_column: Some("email"),
        blind_index_column: Some("email_hash"),
        format: EncryptedColumnFormat::Envelope {
            aad_name: "user.email",
            compressed: false,
        },
    },
    EncryptedColumn {
        table: "user_email_change",
        id_column: "user_id",
        column: "new_email_enc",
        plaintext_column: Some("new_email"),
        blind_index_column: None,
        format: EncryptedColumnFormat::Envelope {
            aad_name: "user_email_change.new_email",
            compressed: false,
        },
    },
    EncryptedColumn {
        table: "integration_connection",
        id_column: "id",
        column: "context_enc",
        plaintext_column: Some("context"),
        blind_index_column: None,
        format: EncryptedColumnFormat::Envelope {
            aad_name: "integration_connection.context",
            compressed: false,
        },
    },
    EncryptedColumn {
        table: "oauth_grant_revocation",
        id_column: "id",
        column: "provider_context_enc",
        plaintext_column: Some("provider_context"),
        blind_index_column: None,
        format: EncryptedColumnFormat::Envelope {
            aad_name: "oauth_grant_revocation.provider_context",
            compressed: false,
        },
    },
    EncryptedColumn {
        table: "third_party_item",
        id_column: "id",
        column: "data_enc",
        plaintext_column: Some("data"),
        blind_index_column: None,
        format: EncryptedColumnFormat::Envelope {
            aad_name: "third_party_item.data",
            compressed: true,
        },
    },
    EncryptedColumn {
        table: "task",
        id_column: "id",
        column: "body_enc",
        plaintext_column: Some("body"),
        blind_index_column: None,
        format: EncryptedColumnFormat::Envelope {
            aad_name: "task.body",
            compressed: false,
        },
    },
    EncryptedColumn {
        table: "user_auth",
        id_column: "id",
        column: "auth_id_token_enc",
        plaintext_column: Some("auth_id_token"),
        blind_index_column: None,
        format: EncryptedColumnFormat::Envelope {
            aad_name: "user_auth.auth_id_token",
            compressed: false,
        },
    },
    EncryptedColumn {
        table: "oauth_credential",
        id_column: "integration_connection_id",
        column: "raw_token_response_enc",
        plaintext_column: Some("raw_token_response"),
        blind_index_column: None,
        format: EncryptedColumnFormat::Envelope {
            aad_name: "oauth_credential.raw_token_response",
            compressed: true,
        },
    },
    EncryptedColumn {
        table: "oauth_credential",
        id_column: "integration_connection_id",
        column: "encrypted_access_token",
        plaintext_column: None,
        blind_index_column: None,
        format: EncryptedColumnFormat::Token,
    },
    EncryptedColumn {
        table: "oauth_credential",
        id_column: "integration_connection_id",
        column: "encrypted_refresh_token",
        plaintext_column: None,
        blind_index_column: None,
        format: EncryptedColumnFormat::Token,
    },
    EncryptedColumn {
        table: "oauth_grant_revocation",
        id_column: "id",
        column: "encrypted_access_token",
        plaintext_column: None,
        blind_index_column: None,
        format: EncryptedColumnFormat::Token,
    },
    EncryptedColumn {
        table: "oauth_grant_revocation",
        id_column: "id",
        column: "encrypted_refresh_token",
        plaintext_column: None,
        blind_index_column: None,
        format: EncryptedColumnFormat::Token,
    },
];

/// Arbitrary constant identifying the data encryption maintenance lock
const DATA_ENCRYPTION_LOCK_ID: i64 = 0x5549_4441_5441_454e; // "UIDATAEN"

fn database_error(err: sqlx::Error, message: String) -> UniversalInboxError {
    UniversalInboxError::DatabaseError {
        source: err,
        message,
    }
}

impl Repository {
    /// Wait for the session-level lock serializing the data encryption maintenance commands
    /// (several instances may start at once). Released with
    /// [`release_data_encryption_lock`](Self::release_data_encryption_lock) or when the
    /// connection closes.
    pub async fn acquire_data_encryption_lock(
        &self,
        connection: &mut PgConnection,
    ) -> Result<(), UniversalInboxError> {
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(DATA_ENCRYPTION_LOCK_ID)
            .execute(connection)
            .await
            .map_err(|err| database_error(err, "Failed to acquire data encryption lock".into()))?;
        Ok(())
    }

    pub async fn release_data_encryption_lock(
        &self,
        connection: &mut PgConnection,
    ) -> Result<(), UniversalInboxError> {
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(DATA_ENCRYPTION_LOCK_ID)
            .execute(connection)
            .await
            .map_err(|err| database_error(err, "Failed to release data encryption lock".into()))?;
        Ok(())
    }

    /// Lock and return up to `limit` rows still holding a plaintext value.
    pub async fn fetch_plaintext_values(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        column: &EncryptedColumn,
        limit: i64,
    ) -> Result<Vec<(Uuid, String)>, UniversalInboxError> {
        let Some(plaintext_column) = column.plaintext_column else {
            return Ok(vec![]);
        };
        let query = format!(
            "SELECT {id} AS id, {plaintext}::TEXT AS plaintext FROM {table} \
             WHERE {plaintext} IS NOT NULL ORDER BY {id} LIMIT $1 FOR UPDATE SKIP LOCKED",
            id = column.id_column,
            plaintext = plaintext_column,
            table = column.table,
        );
        let rows = sqlx::query(AssertSqlSafe(query))
            .bind(limit)
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                database_error(err, format!("Failed to fetch plaintext {}", column.name()))
            })?;
        rows.iter()
            .map(|row| Ok((row.try_get("id")?, row.try_get("plaintext")?)))
            .collect::<Result<_, sqlx::Error>>()
            .map_err(|err| database_error(err, format!("Failed to decode {}", column.name())))
    }

    /// Return up to `limit` encrypted values with an id greater than `after`, in id order,
    /// optionally only those not sealed by the envelope key `except_key_id`.
    pub async fn fetch_encrypted_values(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        column: &EncryptedColumn,
        after: Option<Uuid>,
        except_key_id: Option<KeyId>,
        limit: i64,
    ) -> Result<Vec<(Uuid, Vec<u8>)>, UniversalInboxError> {
        // Token columns may hold legacy values without a key id byte: they are filtered by
        // the caller once decrypted.
        let except_key_id = match column.format {
            EncryptedColumnFormat::Envelope { .. } => except_key_id,
            EncryptedColumnFormat::Token => None,
        };
        let key_id_filter = if except_key_id.is_some() {
            format!("AND get_byte({}, 1) <> $3", column.column)
        } else {
            String::new()
        };
        let query = format!(
            "SELECT {id} AS id, {column} AS value FROM {table} \
             WHERE {column} IS NOT NULL AND ($1::UUID IS NULL OR {id} > $1) {key_id_filter} \
             ORDER BY {id} LIMIT $2 FOR UPDATE",
            id = column.id_column,
            column = column.column,
            table = column.table,
        );
        let mut query = sqlx::query(AssertSqlSafe(query)).bind(after).bind(limit);
        if let Some(except_key_id) = except_key_id {
            query = query.bind(i32::from(except_key_id));
        }
        let rows = query.fetch_all(&mut **executor).await.map_err(|err| {
            database_error(err, format!("Failed to fetch encrypted {}", column.name()))
        })?;
        rows.iter()
            .map(|row| Ok((row.try_get("id")?, row.try_get("value")?)))
            .collect::<Result<_, sqlx::Error>>()
            .map_err(|err| database_error(err, format!("Failed to decode {}", column.name())))
    }

    /// Store `value` in `column` and clear its plaintext column, if any.
    pub async fn store_encrypted_value(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        column: &EncryptedColumn,
        id: Uuid,
        value: &[u8],
        blind_index: Option<&str>,
    ) -> Result<(), UniversalInboxError> {
        let clear_plaintext = column
            .plaintext_column
            .map(|plaintext| format!(", {plaintext} = NULL"))
            .unwrap_or_default();
        let set_blind_index = match (column.blind_index_column, blind_index) {
            (Some(blind_index_column), Some(_)) => format!(", {blind_index_column} = $3"),
            _ => String::new(),
        };
        let query = format!(
            "UPDATE {table} SET {column} = $1{clear_plaintext}{set_blind_index} WHERE {id} = $2",
            table = column.table,
            column = column.column,
            id = column.id_column,
        );
        let mut query = sqlx::query(AssertSqlSafe(query)).bind(value).bind(id);
        if !set_blind_index.is_empty() {
            query = query.bind(blind_index);
        }
        query.execute(&mut **executor).await.map_err(|err| {
            database_error(err, format!("Failed to store encrypted {}", column.name()))
        })?;
        Ok(())
    }

    pub async fn count_plaintext_values(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        column: &EncryptedColumn,
    ) -> Result<i64, UniversalInboxError> {
        let Some(plaintext_column) = column.plaintext_column else {
            return Ok(0);
        };
        let query = format!(
            "SELECT count(*) FROM {table} WHERE {plaintext_column} IS NOT NULL",
            table = column.table,
        );
        sqlx::query_scalar(AssertSqlSafe(query))
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                database_error(err, format!("Failed to count plaintext {}", column.name()))
            })
    }

    /// Count the envelope values of `column` per key id (envelope columns only).
    pub async fn count_values_per_key_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        column: &EncryptedColumn,
    ) -> Result<Vec<(KeyId, i64)>, UniversalInboxError> {
        let query = format!(
            "SELECT get_byte({column}, 1) AS key_id, count(*) AS count FROM {table} \
             WHERE {column} IS NOT NULL GROUP BY 1 ORDER BY 1",
            column = column.column,
            table = column.table,
        );
        let rows = sqlx::query(AssertSqlSafe(query))
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                database_error(err, format!("Failed to count key ids of {}", column.name()))
            })?;
        rows.iter()
            .map(|row| {
                let key_id: i32 = row.try_get("key_id")?;
                Ok((key_id as KeyId, row.try_get("count")?))
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(|err| database_error(err, format!("Failed to decode {}", column.name())))
    }

    /// Lock and return up to `limit` Google `provider_user_id` of `table` still holding an
    /// email address instead of its blind index (a blind index never contains `@`).
    pub async fn fetch_plaintext_google_provider_user_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        table: &str,
        limit: i64,
    ) -> Result<Vec<(Uuid, String)>, UniversalInboxError> {
        let query = format!(
            "SELECT id, provider_user_id FROM {table} \
             WHERE provider_kind::TEXT IN {GOOGLE_PROVIDER_KINDS} AND provider_user_id LIKE '%@%' \
             ORDER BY id LIMIT $1 FOR UPDATE SKIP LOCKED"
        );
        let rows = sqlx::query(AssertSqlSafe(query))
            .bind(limit)
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                database_error(
                    err,
                    format!("Failed to fetch {table} Google provider user ids"),
                )
            })?;
        rows.iter()
            .map(|row| Ok((row.try_get("id")?, row.try_get("provider_user_id")?)))
            .collect::<Result<_, sqlx::Error>>()
            .map_err(|err| {
                database_error(err, format!("Failed to decode {table} provider user ids"))
            })
    }

    pub async fn count_plaintext_google_provider_user_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        table: &str,
    ) -> Result<i64, UniversalInboxError> {
        let query = format!(
            "SELECT count(*) FROM {table} \
             WHERE provider_kind::TEXT IN {GOOGLE_PROVIDER_KINDS} AND provider_user_id LIKE '%@%'"
        );
        sqlx::query_scalar(AssertSqlSafe(query))
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                database_error(
                    err,
                    format!("Failed to count {table} Google provider user ids"),
                )
            })
    }

    pub async fn store_provider_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        table: &str,
        id: Uuid,
        provider_user_id: &str,
    ) -> Result<(), UniversalInboxError> {
        let query = format!("UPDATE {table} SET provider_user_id = $1 WHERE id = $2");
        sqlx::query(AssertSqlSafe(query))
            .bind(provider_user_id)
            .bind(id)
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                database_error(err, format!("Failed to store {table} provider user id"))
            })?;
        Ok(())
    }
}
