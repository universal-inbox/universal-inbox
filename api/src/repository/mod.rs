use std::sync::Arc;

use anyhow::{anyhow, Context};
use sqlx::{pool::PoolConnection, PgPool, Postgres, Transaction};
use tokio::sync::Mutex;

use crate::universal_inbox::UniversalInboxError;

pub mod notification;
pub mod task;

pub struct Repository {
    pub pool: Arc<PgPool>,
}

impl Repository {
    pub fn new(pool: Arc<PgPool>) -> Repository {
        Repository { pool }
    }

    pub async fn connect(&self) -> Result<Arc<ConnectedRepository>, UniversalInboxError> {
        let connection = self
            .pool
            .acquire()
            .await
            .context("Failed to connection to the database")?;
        Ok(Arc::new(ConnectedRepository {
            executor: Arc::new(Mutex::new(connection)),
        }))
    }

    pub async fn begin(&self) -> Result<Arc<TransactionalRepository>, UniversalInboxError> {
        let transaction = self
            .pool
            .begin()
            .await
            .context("Failed to begin database transaction")?;
        Ok(Arc::new(TransactionalRepository {
            executor: Arc::new(Mutex::new(transaction)),
        }))
    }
}

pub struct ConnectedRepository {
    pub executor: Arc<Mutex<PoolConnection<Postgres>>>,
}

pub struct TransactionalRepository<'a> {
    pub executor: Arc<Mutex<Transaction<'a, Postgres>>>,
}

impl<'a> TransactionalRepository<'a> {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn commit(self) -> Result<(), UniversalInboxError> {
        let transaction = Arc::try_unwrap(self.executor)
            .map_err(|_| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Cannot extract transaction to commit it as it has other references using it"
                ))
            })?
            .into_inner();
        Ok(transaction
            .commit()
            .await
            .context("Failed to commit database transaction")?)
    }
}
