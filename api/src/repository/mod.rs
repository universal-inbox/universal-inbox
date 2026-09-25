use std::sync::Arc;

use anyhow::Context;
use sqlx::{FromRow, PgPool, Postgres, Row, Transaction, pool::PoolConnection, postgres::PgRow};
use tracing::error;
use uuid::Uuid;

use crate::universal_inbox::UniversalInboxError;

pub mod auth_token;
pub mod integration_connection;
pub mod notification;
pub mod oauth2;
pub mod oauth_credential;
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
        Ok(self
            .pool
            .acquire()
            .await
            .context("Failed to connection to the database")?)
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        Ok(self
            .pool
            .begin()
            .await
            .context("Failed to begin database transaction")?)
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
