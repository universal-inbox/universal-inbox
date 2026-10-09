use std::sync::Arc;

use tracing::info;

use crate::{
    configuration::Settings,
    repository::Repository,
    universal_inbox::{UniversalInboxError, data_encryption::DataEncryptionService},
    utils::crypto::data_keyring,
};

use super::DataEncryptionCommands;

#[tracing::instrument(name = "data-encryption-command", level = "info", skip_all)]
pub async fn run(
    settings: &Settings,
    command: &DataEncryptionCommands,
) -> Result<(), UniversalInboxError> {
    let pool = Arc::new(
        settings
            .database
            .connect_pool(log::LevelFilter::Debug)
            .await
            .map_err(|err| UniversalInboxError::DatabaseError {
                source: err,
                message: "Failed to connect to Postgresql".to_string(),
            })?,
    );
    let service =
        DataEncryptionService::new(Arc::new(Repository::new(pool)), data_keyring()?.clone());

    let status = match command {
        DataEncryptionCommands::Status => service.status().await?,
        DataEncryptionCommands::EncryptPlaintext { batch_size } => {
            service.encrypt_plaintext(*batch_size).await?
        }
        DataEncryptionCommands::Reencrypt { batch_size } => service.reencrypt(*batch_size).await?,
    };
    info!("Data encryption status:\n{status}");
    println!("{status}");
    Ok(())
}
