use anyhow::{anyhow, Error};
use format_serde_error::SerdeError;
use http::uri::InvalidUri;
use universal_inbox::{integration_connection::ConnectionId, task::TaskId};
use uuid::Uuid;

pub mod integration_connection;
pub mod notification;
pub mod task;
pub mod user;

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{e}\n")?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{cause}")?;
        current = cause.source();
    }
    Ok(())
}

impl std::fmt::Debug for UniversalInboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[derive(thiserror::Error)]
pub enum UniversalInboxError {
    #[error("Error while parsing enum")]
    InvalidEnumData {
        #[source]
        source: enum_derive::ParseEnumError,
        output: String,
    },
    #[error("Error while parsing URI")]
    InvalidUriData {
        #[source]
        source: InvalidUri,
        output: String,
    },
    #[error("Invalid input data: {user_error}")]
    InvalidInputData {
        #[source]
        source: Option<sqlx::Error>,
        user_error: String,
    },
    #[error("The entity {id} already exists")]
    AlreadyExists {
        #[source]
        source: sqlx::Error,
        id: Uuid,
    },
    #[error("Unsupported action: {0}")]
    UnsupportedAction(String),
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),
    #[error("Unauthorized access: {0}")]
    Unauthorized(String),
    #[error("Forbidden access: {0}")]
    Forbidden(String),
    #[error("Unknown Nango connection: {0}")]
    UnknownNangoConnectionError(ConnectionId),
    #[error("An error occured but transaction was still committed: {0}")]
    Recoverable(anyhow::Error),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl UniversalInboxError {
    pub fn from_json_serde_error(serde_error: serde_json::Error, input: String) -> Self {
        if serde_error.to_string().starts_with("missing field") {
            UniversalInboxError::Unexpected(anyhow!("{serde_error}: {input}"))
        } else {
            UniversalInboxError::Unexpected(<SerdeError as Into<Error>>::into(SerdeError::new(
                input,
                serde_error,
            )))
        }
    }
}

#[derive(Debug)]
pub struct UpdateStatus<T> {
    pub updated: bool,
    pub result: Option<T>,
}
