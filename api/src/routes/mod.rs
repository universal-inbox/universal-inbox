mod health_check;
mod notification;

use actix_http::{body::BoxBody, header::TryIntoHeaderValue, StatusCode};
use actix_web::{
    http::header::{self, ContentType},
    HttpResponse, ResponseError,
};
use serde_json::json;

use crate::universal_inbox::UniversalInboxError;

pub use health_check::*;
pub use notification::*;

impl ResponseError for UniversalInboxError {
    fn status_code(&self) -> StatusCode {
        match self {
            UniversalInboxError::InvalidEnumData { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::InvalidUriData { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::MissingInputData(_) => StatusCode::BAD_REQUEST,
            UniversalInboxError::AlreadyExists { .. } => StatusCode::BAD_REQUEST,
            UniversalInboxError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::UnsupportedAction(_) => StatusCode::BAD_REQUEST,
        }
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        let mut res = HttpResponse::new(self.status_code());

        res.headers_mut().insert(
            header::CONTENT_TYPE,
            ContentType::json().try_into_value().unwrap(),
        );

        res.set_body(BoxBody::new(
            json!({ "message": format!("{}", self) }).to_string(),
        ))
    }
}
