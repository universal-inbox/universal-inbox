use std::sync::Arc;

use anyhow::Context;
use sqlx::{PgPool, Postgres, Row, Transaction, pool::PoolConnection, postgres::PgRow};

use crate::universal_inbox::UniversalInboxError;

pub mod auth_token;
pub mod integration_connection;
pub mod notification;
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
