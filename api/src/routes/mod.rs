pub mod auth;
pub mod config;
pub mod health_check;
pub mod integration_connection;
pub mod notification;
pub mod oauth;
pub mod oauth2;
pub mod slack_bridge;
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
use serde_json::json;

use crate::universal_inbox::{UniversalInboxError, UpstreamErrorKind};

const DATABASE_UNAVAILABLE_RETRY_AFTER_SECONDS: &str = "5";

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
            UniversalInboxError::OAuth2InvalidGrant(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::PaymentRequired { .. } => StatusCode::PAYMENT_REQUIRED,
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

        // Pool exhaustion is transient: tell the caller to retry shortly.
        if let UniversalInboxError::DatabaseUnavailable { .. } = self {
            res.headers_mut().insert(
                header::RETRY_AFTER,
                header::HeaderValue::from_static(DATABASE_UNAVAILABLE_RETRY_AFTER_SECONDS),
            );
        }

        // PaymentRequired carries a structured body so the UI can pick the
        // right upgrade copy / modal without reparsing the human message.
        let body = match self {
            UniversalInboxError::PaymentRequired {
                code,
                message,
                details,
            } => json!({
                "message": message,
                "code": code,
                "details": details,
            })
            .to_string(),
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
                .to_string()
            }
            _ => json!({ "message": format!("{self}") }).to_string(),
        };

        res.set_body(BoxBody::new(body))
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
    }
}
