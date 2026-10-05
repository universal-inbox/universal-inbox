use anyhow::{Error, anyhow};
use format_serde_error::SerdeError;
use url::ParseError;
use uuid::Uuid;
use validator::ValidationErrors;

pub mod auth_token;
pub mod integration_connection;
pub mod notification;
pub mod oauth2;
pub mod slack_bridge;
pub mod task;
pub mod third_party;
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
    #[error("Error while parsing URL")]
    InvalidUrlData {
        #[source]
        source: ParseError,
        output: String,
    },
    #[error("Invalid input data: {user_error}")]
    InvalidInputData {
        #[source]
        source: Option<sqlx::Error>,
        user_error: String,
    },
    #[error("Invalid parameters: {0}")]
    InvalidParameters(ValidationErrors),
    #[error("The entity {id} already exists")]
    AlreadyExists {
        #[source]
        source: Option<sqlx::Error>,
        id: Uuid,
    },
    #[error("{0}")]
    Conflict(String),
    #[error("Unsupported action: {0}")]
    UnsupportedAction(String),
    #[error("Item not found: {0}")]
    ItemNotFound(String),
    #[error("Database error: {message}")]
    DatabaseError {
        #[source]
        source: sqlx::Error,
        message: String,
    },
    /// Mapped to HTTP 503 + `Retry-After`. No database connection could be acquired from
    /// the pool within `acquire_timeout` (the pool is exhausted): a transient overload the
    /// caller should retry, not a fault in the request.
    #[error("Database unavailable: {message}")]
    DatabaseUnavailable {
        #[source]
        source: sqlx::Error,
        message: String,
    },
    /// Mapped to HTTP 503 + `Retry-After`. The session revocation store (Redis)
    /// could not be reached: session checks fail closed, so the request is
    /// rejected rather than authenticated without them.
    #[error("Session store unavailable")]
    SessionStoreUnavailable(#[source] anyhow::Error),
    #[error("Unauthorized access: {0}")]
    Unauthorized(anyhow::Error),
    #[error("Forbidden access: {0}")]
    Forbidden(String),
    /// Mapped to HTTP 403 with code `reauthentication_required`: a sensitive
    /// account operation needs a login or re-authentication more recent than
    /// the reauthentication window.
    #[error("Please confirm your identity to continue")]
    ReauthenticationRequired,
    #[error("Too many login attempts. Please try again later.")]
    TooManyLoginAttempts { retry_after_seconds: u64 },
    /// Mapped to HTTP 429 + `Retry-After`. A per-account request budget (e.g.
    /// registration / password-reset emails per address) is exhausted.
    #[error("Too many requests. Please try again later.")]
    TooManyRequests { retry_after_seconds: u64 },
    /// Mapped to HTTP 501. The action needs to send an email but no email
    /// (SMTP) settings are configured on this instance.
    #[error("Email is not configured on this instance")]
    EmailDisabled,
    #[error("Recoverable error: {0}")]
    Recoverable(#[source] anyhow::Error),
    #[error("OAuth2 refresh token is no longer valid (invalid_grant): {0}")]
    OAuth2InvalidGrant(String),
    /// Mapped to HTTP 402. Returned when a Free user attempts an action
    /// gated by their plan (e.g. connecting more integrations than the
    /// `[billing.free_plan]` limit allows). The `code` is a stable,
    /// machine-readable string the web UI uses to pick the right upgrade
    /// modal; `details` (JSON) lets the UI render plan-specific copy
    /// without a second round-trip.
    #[error("Payment required: {message}")]
    PaymentRequired {
        code: &'static str,
        message: String,
        details: serde_json::Value,
    },
    /// An upstream/third-party provider (e.g. the payment provider) returned an
    /// error we want to surface with a faithful HTTP status rather than
    /// collapsing to 500. `kind` selects the status; `code` is a stable,
    /// machine-readable string for the web UI; `message` is human-readable.
    /// Provider-agnostic on purpose so the core error type carries no
    /// dependency on any optional subsystem.
    #[error("{message}")]
    UpstreamServiceError {
        kind: UpstreamErrorKind,
        code: &'static str,
        message: String,
    },
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl UniversalInboxError {
    /// Low-cardinality error class recorded as the `error.type` span attribute
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::InvalidEnumData { .. } => "InvalidEnumData",
            Self::InvalidUrlData { .. } => "InvalidUrlData",
            Self::InvalidInputData { .. } => "InvalidInputData",
            Self::InvalidParameters(_) => "InvalidParameters",
            Self::AlreadyExists { .. } => "AlreadyExists",
            Self::Conflict(_) => "Conflict",
            Self::UnsupportedAction(_) => "UnsupportedAction",
            Self::ItemNotFound(_) => "ItemNotFound",
            Self::DatabaseError { .. } => "DatabaseError",
            Self::DatabaseUnavailable { .. } => "DatabaseUnavailable",
            Self::SessionStoreUnavailable(_) => "SessionStoreUnavailable",
            Self::Unauthorized(_) => "Unauthorized",
            Self::Forbidden(_) => "Forbidden",
            Self::ReauthenticationRequired => "ReauthenticationRequired",
            Self::TooManyLoginAttempts { .. } => "TooManyLoginAttempts",
            Self::TooManyRequests { .. } => "TooManyRequests",
            Self::EmailDisabled => "EmailDisabled",
            Self::Recoverable(_) => "Recoverable",
            Self::OAuth2InvalidGrant(_) => "OAuth2InvalidGrant",
            Self::PaymentRequired { .. } => "PaymentRequired",
            Self::UpstreamServiceError { .. } => "UpstreamServiceError",
            Self::Unexpected(_) => "Unexpected",
        }
    }
}

/// HTTP-status class for [`UniversalInboxError::UpstreamServiceError`]. Keeps
/// the status decision in the producing subsystem while the core type stays
/// free of `actix` / `http` imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamErrorKind {
    /// Caller sent something the provider rejected (400).
    BadRequest,
    /// Payment is required / a card error occurred (402).
    PaymentRequired,
    /// The provider rate-limited us (429).
    RateLimited,
    /// A provider- or transport-side fault (500).
    Internal,
}

impl UniversalInboxError {
    /// True when the error is the caller's fault (mapped to a 4xx HTTP status):
    /// an expected outcome, logged at `warn` and never marking a span as Error.
    /// Must stay in sync with the `ResponseError` status mapping in `routes`.
    pub fn is_client_error(&self) -> bool {
        match self {
            UniversalInboxError::UpstreamServiceError { kind, .. } => {
                *kind != UpstreamErrorKind::Internal
            }
            UniversalInboxError::InvalidInputData { .. }
            | UniversalInboxError::InvalidParameters(_)
            | UniversalInboxError::AlreadyExists { .. }
            | UniversalInboxError::Conflict(_)
            | UniversalInboxError::UnsupportedAction(_)
            | UniversalInboxError::ItemNotFound(_)
            | UniversalInboxError::Unauthorized(_)
            | UniversalInboxError::Forbidden(_)
            | UniversalInboxError::ReauthenticationRequired
            | UniversalInboxError::TooManyLoginAttempts { .. }
            | UniversalInboxError::TooManyRequests { .. }
            | UniversalInboxError::PaymentRequired { .. } => true,
            UniversalInboxError::InvalidEnumData { .. }
            | UniversalInboxError::InvalidUrlData { .. }
            | UniversalInboxError::DatabaseError { .. }
            | UniversalInboxError::DatabaseUnavailable { .. }
            | UniversalInboxError::SessionStoreUnavailable(_)
            | UniversalInboxError::Recoverable(_)
            | UniversalInboxError::OAuth2InvalidGrant(_)
            | UniversalInboxError::EmailDisabled
            | UniversalInboxError::Unexpected(_) => false,
        }
    }

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

    /// True for a Postgres deadlock (`40P01`), serialization failure (`40001`), or
    /// lock-timeout (`55P03`) — errors worth retrying a *short* transaction for, since none
    /// of them indicate a problem with the statement itself, just contention that already
    /// resolved by the time the error surfaced. Never retry a whole sync attempt on this —
    /// only the small atomic claim transactions this crate builds around `FOR UPDATE SKIP
    /// LOCKED` / single-row `UPDATE ... WHERE id = $id`, where a blind retry is cheap and
    /// safe because the statement is idempotent and tiny.
    pub fn is_transient_database_error(&self) -> bool {
        matches!(
            self,
            UniversalInboxError::DatabaseError { source, .. }
                if source
                    .as_database_error()
                    .and_then(|db_error| db_error.code())
                    .is_some_and(|code| matches!(code.as_ref(), "40001" | "40P01" | "55P03"))
        )
    }
}

/// Retries `action` up to 3 times (20ms base, jittered exponential backoff) when it fails
/// with [`UniversalInboxError::is_transient_database_error`]. Intended for the short, atomic
/// claim transactions in this crate (a `FOR UPDATE SKIP LOCKED` claim, or a single-row
/// `UPDATE ... WHERE id = $id`) — never for a whole multi-statement sync attempt, where
/// blindly retrying could repeat non-idempotent work.
pub async fn retry_on_transient_database_error<F, Fut, T>(
    action: F,
) -> Result<T, UniversalInboxError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, UniversalInboxError>>,
{
    tokio_retry::RetryIf::start(
        tokio_retry::strategy::ExponentialBackoff::from_millis(20)
            .map(tokio_retry::strategy::jitter)
            .take(3),
        action,
        |error: &UniversalInboxError| error.is_transient_database_error(),
    )
    .await
}

#[derive(Debug, PartialEq)]
pub struct UpdateStatus<T> {
    pub updated: bool,
    pub result: Option<T>,
}

#[derive(Debug, Clone)]
pub enum UpsertStatus<T: Clone> {
    Created(T),
    Updated { old: T, new: T },
    Untouched(T),
}

impl<T: Clone> UpsertStatus<T> {
    pub fn value(self: UpsertStatus<T>) -> T {
        match self {
            UpsertStatus::Created(inner)
            | UpsertStatus::Updated { new: inner, .. }
            | UpsertStatus::Untouched(inner) => inner,
        }
    }

    pub fn value_ref(self: &UpsertStatus<T>) -> &T {
        match self {
            UpsertStatus::Created(inner)
            | UpsertStatus::Updated { new: inner, .. }
            | UpsertStatus::Untouched(inner) => inner,
        }
    }

    pub fn modified_value(self: UpsertStatus<T>) -> Option<T> {
        match self {
            UpsertStatus::Created(inner) | UpsertStatus::Updated { new: inner, .. } => Some(inner),
            UpsertStatus::Untouched(_) => None,
        }
    }

    pub fn modified_value_ref(self: &UpsertStatus<T>) -> Option<&T> {
        match self {
            UpsertStatus::Created(inner) | UpsertStatus::Updated { new: inner, .. } => Some(inner),
            UpsertStatus::Untouched(_) => None,
        }
    }

    pub fn is_modified(&self) -> bool {
        matches!(
            self,
            UpsertStatus::Created(_) | UpsertStatus::Updated { .. }
        )
    }
}
