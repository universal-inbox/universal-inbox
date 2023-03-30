use std::sync::Arc;

use anyhow::Context;
use sqlx::{pool::PoolConnection, PgPool, Postgres, Transaction};

use crate::universal_inbox::UniversalInboxError;

pub mod notification;
pub mod task;
pub mod user;

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

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        Ok(self
            .pool
            .begin()
            .await
            .context("Failed to begin database transaction")?)
    }
}
