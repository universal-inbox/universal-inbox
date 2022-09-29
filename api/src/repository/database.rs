use crate::universal_inbox::UniversalInboxError;
use anyhow::Context;
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};

#[async_trait]
pub trait TransactionalRepository {
    async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError>;
}

pub struct PgRepository {
    pub pool: PgPool,
}

impl PgRepository {
    pub fn new(pool: PgPool) -> PgRepository {
        PgRepository { pool }
    }
}

#[async_trait]
impl TransactionalRepository for PgRepository {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        Ok(self
            .pool
            .begin()
            .await
            .context("Failed to start new transaction")?)
    }
}
