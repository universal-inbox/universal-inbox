use std::{
    future::{Ready, ready},
    rc::Rc,
    sync::Arc,
};

use actix_web::{
    HttpMessage,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
};
use futures::{FutureExt, future::LocalBoxFuture};
use tracing::warn;

use crate::{
    middlewares::jwt_auth::Authenticated, universal_inbox::user::service::UserService,
    utils::jwt::Claims,
};

/// Middleware recording the activity of authenticated users (any request
/// carrying a valid session or bearer token), which the inactivity policy
/// relies on to pause the integrations of long-gone users.
///
/// Must run after [`super::jwt_auth::AuthenticateMiddleware`], which injects
/// the [`Authenticated`] claims it reads. Recording is best effort: a failure
/// is logged and never fails the request.
#[derive(Clone)]
pub struct RecordUserActivity {
    user_service: Arc<UserService>,
}

impl RecordUserActivity {
    pub fn new(user_service: Arc<UserService>) -> Self {
        Self { user_service }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RecordUserActivity
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = RecordUserActivityMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RecordUserActivityMiddleware {
            service: Rc::new(service),
            user_service: self.user_service.clone(),
        }))
    }
}

pub struct RecordUserActivityMiddleware<S> {
    service: Rc<S>,
    user_service: Arc<UserService>,
}

impl<S, B> Service<ServiceRequest> for RecordUserActivityMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let user_id = req
            .extensions()
            .get::<Authenticated<Claims>>()
            .and_then(|authenticated| authenticated.user_id_opt());
        let service = self.service.clone();
        let user_service = self.user_service.clone();

        async move {
            if let Some(user_id) = user_id
                && let Err(err) = user_service.record_user_activity(user_id).await
            {
                warn!("Failed to record the activity of user {user_id}: {err:?}");
            }
            service.call(req).await
        }
        .boxed_local()
    }
}
