use std::{
    future::{Future, Ready, ready},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use crate::middlewares::jwt_auth::Authenticated;
use actix_web::{
    HttpMessage, HttpResponse,
    body::EitherBody,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use serde_json::json;

use crate::utils::jwt::Claims;

/// Middleware that rejects requests whose JWT carries an `aud` (audience)
/// claim. OAuth2 access tokens issued by `/oauth2/token` set `aud` to the MCP
/// resource URL — they are scoped to MCP and must not be usable on regular API
/// routes (which infer `sub` and grant full account access).
///
/// `exempt_path_prefixes` lets the middleware pass through paths that must
/// continue to accept audienced tokens (e.g. `/api/mcp`). Each exempt prefix
/// is matched against the request path with `starts_with`.
#[derive(Clone, Default)]
pub struct RejectAudiencedTokens {
    exempt_path_prefixes: Rc<Vec<String>>,
}

impl RejectAudiencedTokens {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_exempt_prefixes<I, S>(mut self, prefixes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.exempt_path_prefixes = Rc::new(prefixes.into_iter().map(Into::into).collect());
        self
    }
}

impl<S, B> Transform<S, ServiceRequest> for RejectAudiencedTokens
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = RejectAudiencedTokensMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RejectAudiencedTokensMiddleware {
            service,
            exempt_path_prefixes: self.exempt_path_prefixes.clone(),
        }))
    }
}

pub struct RejectAudiencedTokensMiddleware<S> {
    service: S,
    exempt_path_prefixes: Rc<Vec<String>>,
}

impl<S, B> Service<ServiceRequest> for RejectAudiencedTokensMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path();
        let exempt = self
            .exempt_path_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix.as_str()));

        let has_audienced_token = !exempt
            && req
                .extensions()
                .get::<Authenticated<Claims>>()
                .is_some_and(|auth| auth.claims.aud.is_some());

        if has_audienced_token {
            let response = req
                .into_response(
                    HttpResponse::Unauthorized()
                        .content_type("application/json")
                        .body(
                            json!({
                                "message": "OAuth2 audience-scoped tokens cannot be used on this endpoint"
                            })
                            .to_string(),
                        ),
                )
                .map_into_right_body();
            return Box::pin(async move { Ok(response) });
        }

        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}
