use std::{
    io::{self, IsTerminal},
    pin::pin,
    sync::Arc,
    time::Duration,
};

use tracing::{info, warn};

use crate::{
    configuration::Settings,
    repository::{Repository, data_encryption::GOOGLE_PROVIDER_USER_ID_TABLES},
    universal_inbox::{
        UniversalInboxError,
        data_encryption::{
            DataEncryptionService, DataEncryptionStatus, google_provider_user_id_label,
        },
    },
    utils::crypto::{DataKeyring, data_keyring},
};

use super::DataEncryptionCommands;

mod tui;

/// How the data encryption commands report to the user
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Plain lines and logs, for container logs and pipes
    Plain,
    /// Live progress view and tables, for a terminal
    Interactive,
}

impl OutputMode {
    pub fn detect(plain: bool) -> Self {
        Self::select(
            plain,
            io::stdout().is_terminal(),
            std::env::var("TERM").is_ok_and(|term| term == "dumb"),
        )
    }

    fn select(plain: bool, stdout_is_terminal: bool, term_is_dumb: bool) -> Self {
        if plain || !stdout_is_terminal || term_is_dumb {
            Self::Plain
        } else {
            Self::Interactive
        }
    }
}

#[tracing::instrument(name = "data-encryption-command", level = "info", skip_all)]
pub async fn run(
    settings: &Settings,
    command: &DataEncryptionCommands,
    output_mode: OutputMode,
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
    let keyring = data_keyring()?.clone();
    let service = DataEncryptionService::new(Arc::new(Repository::new(pool)), keyring.clone());

    match output_mode {
        OutputMode::Plain => run_plain(&service, command).await,
        OutputMode::Interactive => run_interactive(&service, &keyring, command).await,
    }
}

async fn run_plain(
    service: &DataEncryptionService,
    command: &DataEncryptionCommands,
) -> Result<(), UniversalInboxError> {
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

async fn run_interactive(
    service: &DataEncryptionService,
    keyring: &DataKeyring,
    command: &DataEncryptionCommands,
) -> Result<(), UniversalInboxError> {
    let status = match command {
        DataEncryptionCommands::Status => service.status().await?,
        DataEncryptionCommands::EncryptPlaintext { batch_size } => {
            let progress = tui::ProgressView::new(
                "Encrypting plaintext values",
                service.plaintext_counts().await?,
            );
            with_progress(
                &progress,
                service.encrypt_plaintext_with_progress(*batch_size, &progress),
            )
            .await?
        }
        DataEncryptionCommands::Reencrypt { batch_size } => {
            let totals = reencrypt_totals(&service.status().await?, keyring);
            let progress = tui::ProgressView::new(
                &format!("Re-encrypting with key {}", keyring.active_key_id()),
                totals,
            );
            with_progress(
                &progress,
                service.reencrypt_with_progress(*batch_size, &progress),
            )
            .await?
        }
    };
    tui::print_status(&status, keyring)
}

/// Number of values `reencrypt` has to process per column: every encrypted value not sealed
/// with the active key (legacy tokens included)
fn reencrypt_totals(status: &DataEncryptionStatus, keyring: &DataKeyring) -> Vec<(String, i64)> {
    let active_key = keyring.active_key_id().to_string();
    let blind_index_columns = GOOGLE_PROVIDER_USER_ID_TABLES
        .iter()
        .map(|table| google_provider_user_id_label(table))
        .collect::<Vec<_>>();
    status
        .columns
        .iter()
        .filter(|column| !blind_index_columns.contains(&column.column))
        .map(|column| {
            let total = column
                .values_per_key
                .iter()
                .filter(|(key, _)| **key != active_key)
                .map(|(_, count)| count)
                .sum();
            (column.column.clone(), total)
        })
        .collect()
}

/// Run `task` while redrawing `progress`, restoring the terminal when it ends, fails or the
/// user interrupts it
async fn with_progress<F>(
    progress: &tui::ProgressView,
    task: F,
) -> Result<DataEncryptionStatus, UniversalInboxError>
where
    F: Future<Output = Result<DataEncryptionStatus, UniversalInboxError>>,
{
    let mut terminal = match progress.terminal() {
        Ok(terminal) => terminal,
        Err(err) => {
            warn!("Cannot show the progress, continuing without it: {err}");
            return task.await;
        }
    };
    let mut task = pin!(task);
    let mut ctrl_c = pin!(tokio::signal::ctrl_c());
    let mut ticker = tokio::time::interval(Duration::from_millis(100));
    let result = loop {
        tokio::select! {
            result = &mut task => break result,
            _ = ticker.tick() => progress.draw(&mut terminal)?,
            _ = &mut ctrl_c => {
                progress.draw(&mut terminal)?;
                drop(terminal);
                eprintln!(
                    "Interrupted: the current batch was rolled back, run the command again to resume"
                );
                std::process::exit(130);
            }
        }
    };
    progress.draw(&mut terminal)?;
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use crate::universal_inbox::data_encryption::{ColumnStatus, LEGACY_TOKEN_KEY_LABEL};

    use super::*;

    #[rstest]
    #[case::terminal(false, true, false, OutputMode::Interactive)]
    #[case::plain_flag(true, true, false, OutputMode::Plain)]
    #[case::not_a_terminal(false, false, false, OutputMode::Plain)]
    #[case::dumb_terminal(false, true, true, OutputMode::Plain)]
    fn test_output_mode_select(
        #[case] plain: bool,
        #[case] stdout_is_terminal: bool,
        #[case] term_is_dumb: bool,
        #[case] expected: OutputMode,
    ) {
        assert_eq!(
            OutputMode::select(plain, stdout_is_terminal, term_is_dumb),
            expected
        );
    }

    /// The plain output is what the container entrypoint logs: keep its format stable
    #[test]
    fn test_plain_status_output() {
        let status = DataEncryptionStatus {
            columns: vec![
                ColumnStatus {
                    column: "task.body_enc".to_string(),
                    plaintext_values: 2,
                    values_per_key: BTreeMap::from([("1".to_string(), 3), ("2".to_string(), 4)]),
                    undecryptable_values: 0,
                },
                ColumnStatus {
                    column: "integration_connection.access_token".to_string(),
                    plaintext_values: 0,
                    values_per_key: BTreeMap::from([(LEGACY_TOKEN_KEY_LABEL.to_string(), 1)]),
                    undecryptable_values: 5,
                },
                ColumnStatus {
                    column: "user.email_enc".to_string(),
                    ..Default::default()
                },
            ],
        };

        assert_eq!(
            status.to_string(),
            "task.body_enc: plaintext: 2, key 1: 3, key 2: 4\n\
             integration_connection.access_token: plaintext: 0, key legacy: 1, undecryptable: 5\n\
             user.email_enc: plaintext: 0, no encrypted value\n"
        );
    }
}
