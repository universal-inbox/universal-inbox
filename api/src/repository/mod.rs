use std::sync::Arc;

use sqlx::{FromRow, PgPool, Postgres, Row, Transaction, pool::PoolConnection, postgres::PgRow};
use tracing::error;
use uuid::Uuid;

use crate::universal_inbox::UniversalInboxError;

pub mod auth_token;
pub mod data_encryption;
pub mod integration_connection;
pub mod notification;
pub mod oauth2;
pub mod oauth_credential;
pub mod oauth_grant_revocation;
pub mod slack_bridge;
pub mod task;
pub mod third_party;
pub mod user;
pub mod user_preferences;

#[derive(Debug)]
pub struct Repository {
    pub pool: Arc<PgPool>,
}

impl Repository {
    pub fn new(pool: Arc<PgPool>) -> Repository {
        Repository { pool }
    }

    pub async fn connect(&self) -> Result<PoolConnection<Postgres>, UniversalInboxError> {
        self.pool
            .acquire()
            .await
            .map_err(|err| pool_error(err, "Failed to connection to the database"))
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.pool
            .begin()
            .await
            .map_err(|err| pool_error(err, "Failed to begin database transaction"))
    }
}

/// A pool acquire timeout is surfaced as the retryable `DatabaseUnavailable` (HTTP 503);
/// any other failure keeps its context and stays unexpected.
fn pool_error(err: sqlx::Error, message: &'static str) -> UniversalInboxError {
    match err {
        sqlx::Error::PoolTimedOut => UniversalInboxError::DatabaseUnavailable {
            source: err,
            message: message.to_string(),
        },
        err => UniversalInboxError::Unexpected(anyhow::Error::new(err).context(message)),
    }
}

trait FromRowWithPrefix<'r, R>: Sized
where
    R: Row,
{
    fn from_row_with_prefix(row: &'r PgRow, prefix: &str) -> sqlx::Result<Self>;
}

/// Decode `rows` one by one, logging and skipping the rows that cannot be decoded.
///
/// Stored JSON data can stop matching the current Rust types (eg. a serde/dependency upgrade
/// changing a third party payload shape). Decoding with `build_query_as().fetch_all()` fails the
/// whole query on the first such row, which takes down a user's whole inbox or sync. Rows are
/// identified in the log by their `id_column` so they can be fixed with a data migration.
pub fn decode_rows_skipping_invalid<T>(rows: &[PgRow], id_column: &str, entity_name: &str) -> Vec<T>
where
    T: for<'r> FromRow<'r, PgRow>,
{
    rows.iter()
        .filter_map(|row| match T::from_row(row) {
            Ok(decoded) => Some(decoded),
            Err(err) => {
                let id = row
                    .try_get::<Uuid, &str>(id_column)
                    .map(|id| id.to_string())
                    .unwrap_or_else(|_| "<unknown id>".to_string());
                error!("Skipping {entity_name} {id} that cannot be decoded: {err}");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_timeout_maps_to_database_unavailable() {
        let err = pool_error(
            sqlx::Error::PoolTimedOut,
            "Failed to begin database transaction",
        );

        assert!(matches!(
            err,
            UniversalInboxError::DatabaseUnavailable {
                source: sqlx::Error::PoolTimedOut,
                ..
            }
        ));
    }

    #[test]
    fn other_pool_errors_stay_unexpected_with_context() {
        let err = pool_error(
            sqlx::Error::PoolClosed,
            "Failed to begin database transaction",
        );

        let UniversalInboxError::Unexpected(err) = err else {
            panic!("expected Unexpected, got {err:?}");
        };
        assert_eq!(err.to_string(), "Failed to begin database transaction");
        assert!(matches!(
            err.downcast_ref::<sqlx::Error>(),
            Some(sqlx::Error::PoolClosed)
        ));
    }
}
