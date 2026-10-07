pub mod auth;
pub mod config;
pub mod health_check;
pub mod integration_connection;
pub mod notification;
pub mod oauth;
pub mod oauth2;
pub mod slack_bridge;
pub mod static_files;
pub mod task;
pub mod third_party;
pub mod user;
pub mod webhook;
pub mod well_known;

use actix_http::{StatusCode, body::BoxBody, header::TryIntoHeaderValue};
use actix_web::{
    HttpResponse, ResponseError,
    http::header::{self, ContentType},
};
use opentelemetry::trace::TraceContextExt;
use serde_json::json;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{
    observability::attr,
    universal_inbox::{UniversalInboxError, UpstreamErrorKind},
};

const DATABASE_UNAVAILABLE_RETRY_AFTER_SECONDS: &str = "5";
pub(crate) const INTERNAL_ERROR_MESSAGE: &str =
    "An internal error occurred. Please retry later or contact support with the correlation id.";
const SERVICE_UNAVAILABLE_MESSAGE: &str =
    "The service is temporarily unavailable. Please retry shortly.";

/// The trace id of the current request span, handed to clients on server
/// errors so a user report can be matched with the full error in traces and
/// logs. `None` when the request is not traced.
pub(crate) fn correlation_id() -> Option<String> {
    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();
    span_context
        .is_valid()
        .then(|| span_context.trace_id().to_string())
}

impl ResponseError for UniversalInboxError {
    fn status_code(&self) -> StatusCode {
        match self {
            UniversalInboxError::UpstreamServiceError { kind, .. } => match kind {
                UpstreamErrorKind::BadRequest => StatusCode::BAD_REQUEST,
                UpstreamErrorKind::PaymentRequired => StatusCode::PAYMENT_REQUIRED,
                UpstreamErrorKind::RateLimited => StatusCode::TOO_MANY_REQUESTS,
                UpstreamErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            },
            UniversalInboxError::InvalidEnumData { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::InvalidUrlData { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::InvalidInputData { .. } => StatusCode::BAD_REQUEST,
            UniversalInboxError::InvalidParameters { .. } => StatusCode::BAD_REQUEST,
            UniversalInboxError::ItemNotFound { .. } => StatusCode::BAD_REQUEST,
            UniversalInboxError::AlreadyExists { .. } => StatusCode::CONFLICT,
            UniversalInboxError::Conflict(_) => StatusCode::CONFLICT,
            UniversalInboxError::Recoverable(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            UniversalInboxError::Forbidden(_) => StatusCode::FORBIDDEN,
            UniversalInboxError::TooManyLoginAttempts { .. } => StatusCode::TOO_MANY_REQUESTS,
            UniversalInboxError::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            UniversalInboxError::UnsupportedAction(_) => StatusCode::BAD_REQUEST,
            UniversalInboxError::DatabaseError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::DatabaseUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            UniversalInboxError::SessionStoreUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            UniversalInboxError::OAuth2InvalidGrant(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::PaymentRequired { .. } => StatusCode::PAYMENT_REQUIRED,
            UniversalInboxError::EmailDisabled => StatusCode::NOT_IMPLEMENTED,
        }
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        let mut res = HttpResponse::new(self.status_code());

        res.headers_mut().insert(
            header::CONTENT_TYPE,
            ContentType::json().try_into_value().unwrap(),
        );

        // Advertise when the caller may retry after a per-account lockout or
        // an exhausted per-account request budget.
        if let UniversalInboxError::TooManyLoginAttempts {
            retry_after_seconds,
        }
        | UniversalInboxError::TooManyRequests {
            retry_after_seconds,
        } = self
            && let Ok(value) = header::HeaderValue::from_str(&retry_after_seconds.to_string())
        {
            res.headers_mut().insert(header::RETRY_AFTER, value);
        }

        // Pool exhaustion and a session store outage are transient: tell the
        // caller to retry shortly.
        if let UniversalInboxError::DatabaseUnavailable { .. }
        | UniversalInboxError::SessionStoreUnavailable(_) = self
        {
            res.headers_mut().insert(
                header::RETRY_AFTER,
                header::HeaderValue::from_static(DATABASE_UNAVAILABLE_RETRY_AFTER_SECONDS),
            );
        }

        // Server errors never expose their details (database or upstream
        // internals) to the client: log them in full here, keyed by the
        // correlation id the client receives.
        let correlation_id = if self.status_code().is_server_error() {
            let correlation_id = correlation_id();
            tracing::error!(
                error = ?self,
                { attr::CORRELATION_ID } = correlation_id.as_deref(),
                "Request failed with a server error"
            );
            correlation_id
        } else {
            None
        };

        // PaymentRequired carries a structured body so the UI can pick the
        // right upgrade copy / modal without reparsing the human message.
        let mut body = match self {
            UniversalInboxError::PaymentRequired {
                code,
                message,
                details,
            } => json!({
                "message": message,
                "code": code,
                "details": details,
            }),
            // Never forward the raw upstream `message` — it can leak provider
            // internals (e.g. "Stripe request failed: …"). Keep the stable,
            // machine-readable `code` and substitute a generic message keyed on
            // the error class so the client gets something actionable without
            // the upstream detail.
            UniversalInboxError::UpstreamServiceError { kind, code, .. } => {
                let message = match kind {
                    UpstreamErrorKind::BadRequest => {
                        "The request was rejected by an upstream service."
                    }
                    UpstreamErrorKind::PaymentRequired => {
                        "Payment is required to complete this request."
                    }
                    UpstreamErrorKind::RateLimited => {
                        "An upstream service is rate-limiting requests. Please retry later."
                    }
                    UpstreamErrorKind::Internal => "An upstream service error occurred.",
                };
                json!({
                    "message": message,
                    "code": code,
                })
            }
            UniversalInboxError::DatabaseUnavailable { .. }
            | UniversalInboxError::SessionStoreUnavailable(_) => {
                json!({ "message": SERVICE_UNAVAILABLE_MESSAGE })
            }
            // A deployment choice, not an internal failure: say so.
            UniversalInboxError::EmailDisabled => json!({ "message": format!("{self}") }),
            _ if self.status_code().is_server_error() => {
                json!({ "message": INTERNAL_ERROR_MESSAGE })
            }
            _ => json!({ "message": format!("{self}") }),
        };
        if let Some(correlation_id) = correlation_id {
            body["correlation_id"] = json!(correlation_id);
        }

        res.set_body(BoxBody::new(body.to_string()))
    }
}

pub async fn option_wildcard() -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::body::MessageBody;
    use universal_inbox::billing::FREE_PLAN_INTEGRATION_LIMIT_CODE;

    #[test]
    fn payment_required_response_uses_402_and_structured_body() {
        let err = UniversalInboxError::PaymentRequired {
            code: FREE_PLAN_INTEGRATION_LIMIT_CODE,
            message: "Free plan allows at most 2 integrations.".to_string(),
            details: json!({
                "current_plan": "free",
                "limit": 2,
                "usage": 2,
            }),
        };

        let res = err.error_response();
        assert_eq!(res.status(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(
            res.headers()
                .get(header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap()),
            Some("application/json")
        );

        let body = res
            .into_body()
            .try_into_bytes()
            .expect("body should be in-memory");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["code"], FREE_PLAN_INTEGRATION_LIMIT_CODE);
        assert_eq!(
            parsed["message"],
            "Free plan allows at most 2 integrations."
        );
        assert_eq!(parsed["details"]["current_plan"], "free");
        assert_eq!(parsed["details"]["limit"], 2);
        assert_eq!(parsed["details"]["usage"], 2);
    }

    #[test]
    fn is_client_error_matches_the_http_status_class() {
        let errors = vec![
            UniversalInboxError::InvalidEnumData {
                source: enum_derive::ParseEnumError,
                output: String::new(),
            },
            UniversalInboxError::InvalidUrlData {
                source: url::ParseError::EmptyHost,
                output: String::new(),
            },
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: String::new(),
            },
            UniversalInboxError::InvalidParameters(validator::ValidationErrors::new()),
            UniversalInboxError::AlreadyExists {
                source: None,
                id: uuid::Uuid::nil(),
            },
            UniversalInboxError::Conflict(String::new()),
            UniversalInboxError::UnsupportedAction(String::new()),
            UniversalInboxError::ItemNotFound(String::new()),
            UniversalInboxError::DatabaseError {
                source: sqlx::Error::PoolTimedOut,
                message: String::new(),
            },
            UniversalInboxError::DatabaseUnavailable {
                source: sqlx::Error::PoolTimedOut,
                message: String::new(),
            },
            UniversalInboxError::SessionStoreUnavailable(anyhow::anyhow!("")),
            UniversalInboxError::Unauthorized(anyhow::anyhow!("")),
            UniversalInboxError::Forbidden(String::new()),
            UniversalInboxError::TooManyLoginAttempts {
                retry_after_seconds: 1,
            },
            UniversalInboxError::TooManyRequests {
                retry_after_seconds: 1,
            },
            UniversalInboxError::Recoverable(anyhow::anyhow!("")),
            UniversalInboxError::OAuth2InvalidGrant(String::new()),
            UniversalInboxError::PaymentRequired {
                code: "",
                message: String::new(),
                details: json!({}),
            },
            UniversalInboxError::EmailDisabled,
            UniversalInboxError::Unexpected(anyhow::anyhow!("")),
        ]
        .into_iter()
        .chain(
            [
                UpstreamErrorKind::BadRequest,
                UpstreamErrorKind::PaymentRequired,
                UpstreamErrorKind::RateLimited,
                UpstreamErrorKind::Internal,
            ]
            .into_iter()
            .map(|kind| UniversalInboxError::UpstreamServiceError {
                kind,
                code: "",
                message: String::new(),
            }),
        );

        for err in errors {
            assert_eq!(
                err.is_client_error(),
                err.status_code().is_client_error(),
                "{} is_client_error does not match its HTTP status {}",
                err.error_type(),
                err.status_code()
            );
        }
    }

    #[test]
    fn database_unavailable_response_uses_503_and_retry_after() {
        let err = UniversalInboxError::DatabaseUnavailable {
            source: sqlx::Error::PoolTimedOut,
            message: "Failed to begin database transaction".to_string(),
        };

        let res = err.error_response();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            res.headers()
                .get(header::RETRY_AFTER)
                .map(|v| v.to_str().unwrap()),
            Some(DATABASE_UNAVAILABLE_RETRY_AFTER_SECONDS)
        );
        let body = response_body(res);
        assert_eq!(body["message"], SERVICE_UNAVAILABLE_MESSAGE);
        assert!(!body.to_string().contains("transaction"));
    }

    #[test]
    fn session_store_unavailable_response_uses_503_and_retry_after() {
        let err = UniversalInboxError::SessionStoreUnavailable(anyhow::anyhow!(
            "Failed to check revoked session: redis timed out"
        ));

        let res = err.error_response();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            res.headers()
                .get(header::RETRY_AFTER)
                .map(|v| v.to_str().unwrap()),
            Some(DATABASE_UNAVAILABLE_RETRY_AFTER_SECONDS)
        );
        let body = response_body(res);
        assert_eq!(body["message"], SERVICE_UNAVAILABLE_MESSAGE);
        assert!(!body.to_string().contains("redis"));
    }

    fn response_body(res: HttpResponse<BoxBody>) -> serde_json::Value {
        let body = res
            .into_body()
            .try_into_bytes()
            .expect("body should be in-memory");
        serde_json::from_slice(&body).unwrap()
    }

    #[test]
    fn server_error_response_hides_error_details() {
        let errors = [
            UniversalInboxError::DatabaseError {
                source: sqlx::Error::Protocol("relation \"secret_table\" does not exist".into()),
                message: "Failed to fetch secret_table row".to_string(),
            },
            UniversalInboxError::Unexpected(
                anyhow::anyhow!("secret_table").context("Failed to call upstream"),
            ),
            UniversalInboxError::Recoverable(anyhow::anyhow!("secret_table")),
        ];

        for err in errors {
            let res = err.error_response();
            assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let body = response_body(res);
            assert_eq!(body["message"], INTERNAL_ERROR_MESSAGE);
            assert!(!body.to_string().contains("secret_table"));
            // Untraced: no correlation id to hand out.
            assert!(body.get("correlation_id").is_none());
        }
    }

    #[test]
    fn server_error_response_carries_the_trace_id_as_correlation_id() {
        use opentelemetry::trace::TracerProvider;
        use tracing_subscriber::layer::SubscriberExt;

        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("request");
            let _guard = span.enter();
            let trace_id = span.context().span().span_context().trace_id().to_string();

            let res = UniversalInboxError::Unexpected(anyhow::anyhow!("boom")).error_response();

            let body = response_body(res);
            assert_eq!(body["message"], INTERNAL_ERROR_MESSAGE);
            assert_eq!(body["correlation_id"], trace_id);
        });
    }

    #[test]
    fn client_error_response_keeps_its_message() {
        let res = UniversalInboxError::InvalidInputData {
            source: None,
            user_error: "Name is required".to_string(),
        }
        .error_response();

        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = response_body(res);
        assert_eq!(body["message"], "Invalid input data: Name is required");
        assert!(body.get("correlation_id").is_none());
    }
}
