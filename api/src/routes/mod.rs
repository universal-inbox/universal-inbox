pub mod auth;
pub mod config;
pub mod health_check;
pub mod integration_connection;
pub mod notification;
pub mod subscription;
pub mod task;
pub mod third_party;
pub mod user;
pub mod webhook;

use std::{
    future::{Future, Ready, ready},
    pin::Pin,
    sync::Arc,
};

use actix_http::{Method, StatusCode, body::BoxBody, header::TryIntoHeaderValue};
use actix_web::{
    Error, HttpMessage, HttpResponse, ResponseError,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    http::header::{self, ContentType},
};
use serde_json::json;

use crate::{subscription::service::SubscriptionService, universal_inbox::UniversalInboxError};

use universal_inbox::user::UserId;

/// Middleware to enforce read-only mode for users with expired subscriptions.
/// Blocks POST, PUT, PATCH, DELETE requests for users in read-only mode.
pub struct WriteAccessMiddleware {
    subscription_service: Arc<SubscriptionService>,
}

impl WriteAccessMiddleware {
    pub fn new(subscription_service: Arc<SubscriptionService>) -> Self {
        Self {
            subscription_service,
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for WriteAccessMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<actix_http::body::EitherBody<B>>;
    type Error = Error;
    type Transform = WriteAccessMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(WriteAccessMiddlewareService {
            service,
            subscription_service: self.subscription_service.clone(),
        }))
    }
}

pub struct WriteAccessMiddlewareService<S> {
    service: S,
    subscription_service: Arc<SubscriptionService>,
}

impl<S, B> Service<ServiceRequest> for WriteAccessMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<actix_http::body::EitherBody<B>>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let method = req.method().clone();
        let is_write_operation = matches!(
            method,
            Method::POST | Method::PUT | Method::PATCH | Method::DELETE
        );

        if !is_write_operation {
            let fut = self.service.call(req);
            return Box::pin(async move {
                let res = fut.await?;
                Ok(res.map_into_left_body())
            });
        }

        let user_id: Option<UserId> = req
            .extensions()
            .get::<crate::utils::jwt::Claims>()
            .and_then(|claims| claims.sub.parse::<UserId>().ok());

        let subscription_service = self.subscription_service.clone();
        let fut = self.service.call(req);

        Box::pin(async move {
            if let Some(user_id) = user_id {
                let mut transaction = match subscription_service.begin().await {
                    Ok(tx) => tx,
                    Err(e) => {
                        tracing::error!(
                            "Failed to begin transaction for write access check: {}",
                            e
                        );
                        let res = fut.await?;
                        return Ok(res.map_into_left_body());
                    }
                };

                match subscription_service
                    .is_read_only_mode(&mut transaction, user_id)
                    .await
                {
                    Ok(true) => {
                        tracing::info!(
                            user_id = %user_id,
                            "Blocking write operation for user in read-only mode"
                        );

                        let response = HttpResponse::Forbidden()
                            .content_type("application/json")
                            .body(
                            json!({
                                "error": "subscription_required",
                                "message": "Your trial has expired. Please subscribe to continue."
                            })
                            .to_string(),
                        );

                        if let Err(e) = transaction.commit().await {
                            tracing::error!(
                                "Failed to commit transaction after read-only check: {}",
                                e
                            );
                        }

                        return Ok(ServiceResponse::new(fut.await?.into_parts().0, response)
                            .map_into_right_body());
                    }
                    Ok(false) => {
                        if let Err(e) = transaction.commit().await {
                            tracing::error!(
                                "Failed to commit transaction after read-only check: {}",
                                e
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to check read-only mode: {}", e);
                        if let Err(rollback_err) = transaction.rollback().await {
                            tracing::error!("Failed to rollback transaction: {}", rollback_err);
                        }
                    }
                }
            }

            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}

impl ResponseError for UniversalInboxError {
    fn status_code(&self) -> StatusCode {
        match self {
            UniversalInboxError::InvalidEnumData { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::InvalidUrlData { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::InvalidInputData { .. } => StatusCode::BAD_REQUEST,
            UniversalInboxError::InvalidParameters { .. } => StatusCode::BAD_REQUEST,
            UniversalInboxError::ItemNotFound { .. } => StatusCode::BAD_REQUEST,
            UniversalInboxError::AlreadyExists { .. } => StatusCode::CONFLICT,
            UniversalInboxError::Recoverable(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UniversalInboxError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            UniversalInboxError::Forbidden(_) => StatusCode::FORBIDDEN,
            UniversalInboxError::UnsupportedAction(_) => StatusCode::BAD_REQUEST,
            UniversalInboxError::DatabaseError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        let mut res = HttpResponse::new(self.status_code());

        res.headers_mut().insert(
            header::CONTENT_TYPE,
            ContentType::json().try_into_value().unwrap(),
        );

        res.set_body(BoxBody::new(
            json!({ "message": format!("{self}") }).to_string(),
        ))
    }
}

pub async fn option_wildcard() -> HttpResponse {
    HttpResponse::Ok().finish()
}
