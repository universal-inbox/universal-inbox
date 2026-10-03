//! In-repo JWT authentication middleware for actix-web.
//!
//! Replaces the abandoned `actix-jwt-authc` crate (upstream's last release was 0.2.0 in
//! July 2022; we only depended on a personal fork to keep it compiling). The middleware
//! extracts a JWT from the `Authorization` header or the `actix-session` cookie, validates
//! it with `jsonwebtoken`, and injects an [`Authenticated`] value into the request
//! extensions. Route handlers read it through the [`Authenticated`] / [`MaybeAuthenticated`]
//! extractors, and downstream middlewares ([`super::audience_guard`], MCP) read it directly
//! from the request extensions.
//!
//! Tokens read from the `Authorization` header are additionally passed to an optional
//! [`BearerTokenChecker`], which rejects stored API tokens that were revoked or have
//! expired. Session JWTs are not stored: logout clears the session cookie
//! (`session.purge()`), and OAuth2 access is revoked by deleting refresh tokens.
//! Tokens read from the session cookie are passed to an optional
//! [`SessionTokenChecker`], which rejects sessions issued before the user's sessions
//! were revoked (password change or reset).

use std::{
    future::{Ready, ready},
    marker::PhantomData,
    rc::Rc,
    sync::Arc,
};

use actix_session::SessionExt;
use actix_web::{
    FromRequest, HttpMessage,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
};
use anyhow::{Context, anyhow};
use futures::{FutureExt, future::LocalBoxFuture};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use universal_inbox::user::UserId;

use crate::{universal_inbox::UniversalInboxError, utils::jwt::Claims};

/// A wrapper around a raw JWT string.
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct JWT(pub String);

/// Decides whether a validly signed bearer token is still accepted, typically by
/// consulting its stored row (revocation, expiry).
pub trait BearerTokenChecker {
    fn is_active(&self, jwt: JWT) -> LocalBoxFuture<'static, Result<bool, UniversalInboxError>>;
}

/// Decides whether a validly signed session JWT (read from the session cookie) is
/// still accepted, given its subject and issue time.
pub trait SessionTokenChecker {
    fn is_active(
        &self,
        subject: String,
        issued_at: i64,
    ) -> LocalBoxFuture<'static, Result<bool, UniversalInboxError>>;
}

/// The claims a [`SessionTokenChecker`] needs from a decoded JWT.
pub trait SessionClaims {
    fn subject(&self) -> &str;
    /// Issue time, in seconds since the Unix epoch.
    fn issued_at(&self) -> i64;
}

impl SessionClaims for Claims {
    fn subject(&self) -> &str {
        &self.sub
    }

    fn issued_at(&self) -> i64 {
        self.iat as i64
    }
}

/// The key under which the JWT is stored in the [`actix_session::Session`] cookie.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct JWTSessionKey(pub String);

/// "Must-be-authenticated" marker injected into request extensions by
/// [`AuthenticateMiddleware`]. Used as a route extractor (401 when absent) and read
/// directly from request extensions by downstream middlewares.
///
/// Generic over the claims type so callers can use their own JWT claims struct.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Authenticated<T> {
    pub jwt: JWT,
    pub claims: T,
}

impl<T> FromRequest for Authenticated<T>
where
    T: Clone + 'static,
{
    type Error = UniversalInboxError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        ready(
            req.extensions()
                .get::<Authenticated<T>>()
                .cloned()
                .ok_or_else(|| UniversalInboxError::Unauthorized(anyhow!("Unauthenticated"))),
        )
    }
}

impl Authenticated<Claims> {
    /// Parse the `sub` claim into a [`UserId`], mapping a malformed value to an
    /// internal error.
    pub fn user_id(&self) -> Result<UserId, UniversalInboxError> {
        self.claims
            .sub
            .parse::<UserId>()
            .context("Wrong user ID format")
            .map_err(UniversalInboxError::Unexpected)
    }

    /// Best-effort variant for call sites that only want `Some(user_id)` and
    /// deliberately ignore a malformed subject (logging / rate-limiting).
    pub fn user_id_opt(&self) -> Option<UserId> {
        self.claims.sub.parse::<UserId>().ok()
    }
}

/// "Might-be-authenticated" marker. As an extractor it never fails: it resolves to
/// [`MaybeAuthenticated::None`] for unauthenticated requests.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum MaybeAuthenticated<T> {
    Just(Authenticated<T>),
    None,
}

impl<T> MaybeAuthenticated<T> {
    pub fn into_option(self) -> Option<Authenticated<T>> {
        self.into()
    }
}

impl<T> From<MaybeAuthenticated<T>> for Option<Authenticated<T>> {
    fn from(maybe_authenticated: MaybeAuthenticated<T>) -> Self {
        match maybe_authenticated {
            MaybeAuthenticated::Just(v) => Some(v),
            MaybeAuthenticated::None => None,
        }
    }
}

impl<T> FromRequest for MaybeAuthenticated<T>
where
    T: Clone + 'static,
{
    type Error = UniversalInboxError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        ready(Ok(
            match req.extensions().get::<Authenticated<T>>().cloned() {
                Some(v) => MaybeAuthenticated::Just(v),
                None => MaybeAuthenticated::None,
            },
        ))
    }
}

/// Settings controlling how the authentication middleware extracts and validates JWTs.
#[derive(Clone)]
pub struct AuthenticateMiddlewareSettings {
    /// Decoding key used to verify the JWT signature.
    pub jwt_decoding_key: DecodingKey,

    /// `jsonwebtoken` validation options.
    pub jwt_validator: Validation,

    /// Optional key for extracting a JWT from the request's [`actix_session::Session`].
    /// When `None`, sessions are not consulted.
    pub jwt_session_key: Option<JWTSessionKey>,

    /// Optional `Authorization` header prefixes (e.g. `"Bearer"`). When `None`, the
    /// `Authorization` header is not consulted.
    pub jwt_authorization_header_prefixes: Option<Vec<String>>,

    /// Optional check applied to tokens read from the `Authorization` header after
    /// their signature is validated. When `None`, bearer tokens are not checked further.
    pub bearer_token_checker: Option<Arc<dyn BearerTokenChecker + Send + Sync>>,

    /// Optional check applied to tokens read from the session cookie after their
    /// signature is validated. When `None`, session tokens are not checked further.
    pub session_token_checker: Option<Arc<dyn SessionTokenChecker + Send + Sync>>,
}

/// Factory for [`AuthenticateMiddleware`]. Instantiate once at bootstrap and clone into the
/// app factory closure; cloning is cheap (internally `Arc`-backed).
#[derive(Clone)]
pub struct AuthenticateMiddlewareFactory<ClaimsType> {
    jwt_decoding_key: Arc<DecodingKey>,
    jwt_validator: Arc<Validation>,
    jwt_session_key: Option<Arc<JWTSessionKey>>,
    /// Header prefixes are pre-suffixed with a space (e.g. `"Bearer "`) for prefix stripping.
    jwt_authorization_header_prefixes: Option<Arc<Vec<String>>>,
    bearer_token_checker: Option<Arc<dyn BearerTokenChecker + Send + Sync>>,
    session_token_checker: Option<Arc<dyn SessionTokenChecker + Send + Sync>>,
    _claims_type_marker: PhantomData<ClaimsType>,
}

impl<ClaimsType> AuthenticateMiddlewareFactory<ClaimsType> {
    pub fn new(
        settings: AuthenticateMiddlewareSettings,
    ) -> AuthenticateMiddlewareFactory<ClaimsType> {
        AuthenticateMiddlewareFactory {
            jwt_decoding_key: Arc::new(settings.jwt_decoding_key),
            jwt_validator: Arc::new(settings.jwt_validator),
            jwt_session_key: settings.jwt_session_key.map(Arc::new),
            jwt_authorization_header_prefixes: settings.jwt_authorization_header_prefixes.map(
                |prefixes| Arc::new(prefixes.iter().map(|prefix| format!("{prefix} ")).collect()),
            ),
            bearer_token_checker: settings.bearer_token_checker,
            session_token_checker: settings.session_token_checker,
            _claims_type_marker: PhantomData,
        }
    }
}

impl<S, B, ClaimsType> Transform<S, ServiceRequest> for AuthenticateMiddlewareFactory<ClaimsType>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    ClaimsType: DeserializeOwned + SessionClaims + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Transform = AuthenticateMiddleware<S, ClaimsType>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthenticateMiddleware {
            service: Rc::new(service),
            jwt_decoding_key: self.jwt_decoding_key.clone(),
            jwt_validator: self.jwt_validator.clone(),
            jwt_session_key: self.jwt_session_key.clone(),
            jwt_authorization_header_prefixes: self.jwt_authorization_header_prefixes.clone(),
            bearer_token_checker: self.bearer_token_checker.clone(),
            session_token_checker: self.session_token_checker.clone(),
            _claims_type_marker: PhantomData,
        }))
    }
}

/// Extracts a JWT from each request, validates it, and injects [`Authenticated`] into the
/// request extensions on success.
pub struct AuthenticateMiddleware<S, ClaimsType> {
    service: Rc<S>,
    jwt_decoding_key: Arc<DecodingKey>,
    jwt_validator: Arc<Validation>,
    jwt_session_key: Option<Arc<JWTSessionKey>>,
    jwt_authorization_header_prefixes: Option<Arc<Vec<String>>>,
    bearer_token_checker: Option<Arc<dyn BearerTokenChecker + Send + Sync>>,
    session_token_checker: Option<Arc<dyn SessionTokenChecker + Send + Sync>>,
    _claims_type_marker: PhantomData<ClaimsType>,
}

impl<S, B, ClaimsType> Service<ServiceRequest> for AuthenticateMiddleware<S, ClaimsType>
where
    ClaimsType: DeserializeOwned + SessionClaims + 'static,
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();
        let jwt_decoding_key = self.jwt_decoding_key.clone();
        let jwt_validator = self.jwt_validator.clone();
        let jwt_session_key = self.jwt_session_key.clone();
        let jwt_authorization_header_prefixes = self.jwt_authorization_header_prefixes.clone();
        let bearer_token_checker = self.bearer_token_checker.clone();
        let session_token_checker = self.session_token_checker.clone();
        async move {
            authenticate::<S, B, ClaimsType>(
                svc,
                req,
                &jwt_decoding_key,
                jwt_session_key,
                jwt_authorization_header_prefixes,
                bearer_token_checker,
                session_token_checker,
                &jwt_validator,
            )
            .await
        }
        .boxed_local()
    }
}

#[allow(clippy::too_many_arguments)]
async fn authenticate<S, B, ClaimsType>(
    svc: Rc<S>,
    req: ServiceRequest,
    jwt_decoding_key: &DecodingKey,
    jwt_session_key: Option<Arc<JWTSessionKey>>,
    jwt_authorization_header_prefixes: Option<Arc<Vec<String>>>,
    bearer_token_checker: Option<Arc<dyn BearerTokenChecker + Send + Sync>>,
    session_token_checker: Option<Arc<dyn SessionTokenChecker + Send + Sync>>,
    validation: &Validation,
) -> Result<ServiceResponse<EitherBody<B>>, actix_web::Error>
where
    ClaimsType: DeserializeOwned + SessionClaims + 'static,
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
{
    let maybe_bearer_jwt =
        jwt_authorization_header_prefixes.and_then(|prefixes| extract_bearer_jwt(&req, &prefixes));
    let is_bearer = maybe_bearer_jwt.is_some();
    let maybe_extracted_jwt = maybe_bearer_jwt
        .or_else(|| jwt_session_key.and_then(|key| extract_session_jwt(&req, &key)));

    if let Some(jwt) = maybe_extracted_jwt {
        match decode::<ClaimsType>(jwt.0.as_str(), jwt_decoding_key, validation) {
            Ok(decoded) => {
                let is_active = if is_bearer {
                    match bearer_token_checker {
                        Some(checker) => checker.is_active(jwt.clone()).await,
                        None => Ok(true),
                    }
                } else {
                    match session_token_checker {
                        Some(checker) => {
                            checker
                                .is_active(
                                    decoded.claims.subject().to_string(),
                                    decoded.claims.issued_at(),
                                )
                                .await
                        }
                        None => Ok(true),
                    }
                };
                match is_active {
                    Ok(true) => {}
                    Ok(false) => {
                        let response = req
                            .error_response(UniversalInboxError::Unauthorized(anyhow!(
                                "Invalid session: token has been revoked or has expired"
                            )))
                            .map_into_right_body();
                        return Ok(response);
                    }
                    Err(error) => {
                        let response = req.error_response(error).map_into_right_body();
                        return Ok(response);
                    }
                }
                req.extensions_mut().insert(Authenticated {
                    jwt,
                    claims: decoded.claims,
                });
            }
            Err(error) => {
                let response = req
                    .error_response(UniversalInboxError::Unauthorized(anyhow!(
                        "Invalid session: {error}"
                    )))
                    .map_into_right_body();
                return Ok(response);
            }
        }
    }

    let res = svc.call(req).await?;
    Ok(res.map_into_left_body())
}

fn extract_bearer_jwt(req: &ServiceRequest, auth_prefixes: &[String]) -> Option<JWT> {
    let authorization_header = req.headers().get("Authorization")?;
    let as_str = authorization_header.to_str().ok()?;
    let jwt_str = auth_prefixes
        .iter()
        .filter_map(|prefix| as_str.strip_prefix(prefix))
        .next()?;
    Some(JWT(jwt_str.to_string()))
}

fn extract_session_jwt(req: &ServiceRequest, jwt_session_key: &JWTSessionKey) -> Option<JWT> {
    let session = req.get_session();
    let jwt_str = session.get::<String>(&jwt_session_key.0).ok().flatten()?;
    Some(JWT(jwt_str))
}

#[cfg(test)]
mod tests {
    use actix_http::Request;
    use actix_session::{Session, SessionMiddleware, storage::CookieSessionStore};
    use actix_web::{
        App, HttpResponse,
        body::{BoxBody, EitherBody},
        cookie::Key,
        dev::ServiceResponse,
        get, test, web,
    };
    use chrono::Utc;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use ring::{
        rand::SystemRandom,
        signature::{Ed25519KeyPair, KeyPair},
    };
    use serde::{Deserialize, Serialize};

    use super::*;

    const ALGO: Algorithm = Algorithm::EdDSA;
    const SESSION_KEY: &str = "jwt-session";

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct TestClaims {
        exp: usize,
        iat: usize,
        sub: String,
    }

    impl SessionClaims for TestClaims {
        fn subject(&self) -> &str {
            &self.sub
        }

        fn issued_at(&self) -> i64 {
            self.iat as i64
        }
    }

    fn generate_keys() -> (EncodingKey, DecodingKey) {
        let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap();
        (
            EncodingKey::from_ed_der(doc.as_ref()),
            DecodingKey::from_ed_der(keypair.public_key().as_ref()),
        )
    }

    /// Builds a token for `sub`, expiring `exp_offset_secs` from now (negative = expired).
    fn make_token(encoding_key: &EncodingKey, sub: &str, exp_offset_secs: i64) -> String {
        let now = Utc::now().timestamp();
        let claims = TestClaims {
            iat: now as usize,
            exp: (now + exp_offset_secs) as usize,
            sub: sub.to_string(),
        };
        encode(&Header::new(ALGO), &claims, encoding_key).unwrap()
    }

    fn settings(decoding_key: DecodingKey) -> AuthenticateMiddlewareSettings {
        let mut validator = Validation::new(ALGO);
        // Session/test tokens carry no `aud`, matching the production config.
        validator.validate_aud = false;
        AuthenticateMiddlewareSettings {
            jwt_decoding_key: decoding_key,
            jwt_validator: validator,
            jwt_session_key: Some(JWTSessionKey(SESSION_KEY.to_string())),
            jwt_authorization_header_prefixes: Some(vec!["Bearer".to_string()]),
            bearer_token_checker: None,
            session_token_checker: None,
        }
    }

    #[get("/protected")]
    async fn protected(
        auth: Authenticated<TestClaims>,
    ) -> Result<HttpResponse, UniversalInboxError> {
        Ok(HttpResponse::Ok().json(auth.claims))
    }

    #[get("/maybe")]
    async fn maybe(auth: MaybeAuthenticated<TestClaims>) -> HttpResponse {
        match auth.into_option() {
            Some(authenticated) => HttpResponse::Ok().json(authenticated.claims.sub),
            None => HttpResponse::Ok().json("anonymous"),
        }
    }

    /// Stores `token` (when provided) in the session cookie, mimicking the real login flow.
    #[get("/login")]
    async fn login(session: Session, token: web::Data<Option<String>>) -> HttpResponse {
        if let Some(token) = token.get_ref() {
            session.insert(SESSION_KEY, token).unwrap();
        }
        HttpResponse::Ok().finish()
    }

    async fn init_app(
        decoding_key: DecodingKey,
        session_login_token: Option<String>,
    ) -> impl Service<Request, Response = ServiceResponse<EitherBody<BoxBody>>, Error = actix_web::Error>
    {
        init_app_with_settings(settings(decoding_key), session_login_token).await
    }

    async fn init_app_with_settings(
        settings: AuthenticateMiddlewareSettings,
        session_login_token: Option<String>,
    ) -> impl Service<Request, Response = ServiceResponse<EitherBody<BoxBody>>, Error = actix_web::Error>
    {
        let factory = AuthenticateMiddlewareFactory::<TestClaims>::new(settings);
        test::init_service(
            App::new()
                .app_data(web::Data::new(session_login_token))
                // `wrap` is LIFO: SessionMiddleware (declared last) runs first so the auth
                // middleware can read the session cookie — same ordering as production.
                .wrap(factory)
                .wrap(
                    SessionMiddleware::builder(CookieSessionStore::default(), Key::generate())
                        .cookie_secure(false)
                        .build(),
                )
                .service(protected)
                .service(maybe)
                .service(login),
        )
        .await
    }

    // --- header extraction unit tests ---

    #[actix_web::test]
    async fn extract_bearer_jwt_returns_none_without_header() {
        let req = test::TestRequest::default().to_srv_request();
        assert!(extract_bearer_jwt(&req, &["Bearer ".to_string()]).is_none());
    }

    #[actix_web::test]
    async fn extract_bearer_jwt_strips_prefix() {
        let req = test::TestRequest::default()
            .insert_header(("Authorization", "Bearer XYZ"))
            .to_srv_request();
        assert_eq!(
            Some(JWT("XYZ".to_string())),
            extract_bearer_jwt(&req, &["Bearer ".to_string()])
        );
    }

    #[actix_web::test]
    async fn extract_bearer_jwt_rejects_wrong_prefix() {
        let req = test::TestRequest::default()
            .insert_header(("Authorization", "ApiKey XYZ"))
            .to_srv_request();
        assert!(extract_bearer_jwt(&req, &["Bearer ".to_string()]).is_none());
    }

    #[actix_web::test]
    async fn extract_session_jwt_returns_none_without_session() {
        let req = test::TestRequest::default().to_srv_request();
        assert!(extract_session_jwt(&req, &JWTSessionKey(SESSION_KEY.to_string())).is_none());
    }

    // --- middleware integration tests ---

    #[actix_web::test]
    async fn protected_route_rejects_unauthenticated_request() {
        let (_enc, dec) = generate_keys();
        let app = init_app(dec, None).await;
        let resp = test::call_service(
            &app,
            test::TestRequest::get().uri("/protected").to_request(),
        )
        .await;
        assert_eq!(actix_http::StatusCode::UNAUTHORIZED, resp.status());
    }

    #[actix_web::test]
    async fn maybe_route_allows_unauthenticated_request() {
        let (_enc, dec) = generate_keys();
        let app = init_app(dec, None).await;
        let resp =
            test::call_service(&app, test::TestRequest::get().uri("/maybe").to_request()).await;
        assert_eq!(actix_http::StatusCode::OK, resp.status());
        let body: String = test::read_body_json(resp).await;
        assert_eq!("anonymous", body);
    }

    #[actix_web::test]
    async fn valid_bearer_token_is_authenticated() {
        let (enc, dec) = generate_keys();
        let token = make_token(&enc, "user-123", 3600);
        let app = init_app(dec, None).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(actix_http::StatusCode::OK, resp.status());
        let claims: TestClaims = test::read_body_json(resp).await;
        assert_eq!("user-123", claims.sub);
    }

    #[actix_web::test]
    async fn maybe_route_reads_bearer_token() {
        let (enc, dec) = generate_keys();
        let token = make_token(&enc, "user-abc", 3600);
        let app = init_app(dec, None).await;
        let req = test::TestRequest::get()
            .uri("/maybe")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(actix_http::StatusCode::OK, resp.status());
        let sub: String = test::read_body_json(resp).await;
        assert_eq!("user-abc", sub);
    }

    #[actix_web::test]
    async fn garbage_bearer_token_is_rejected() {
        let (_enc, dec) = generate_keys();
        let app = init_app(dec, None).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", "Bearer not-a-jwt"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(actix_http::StatusCode::UNAUTHORIZED, resp.status());
    }

    #[actix_web::test]
    async fn expired_bearer_token_is_rejected() {
        let (enc, dec) = generate_keys();
        let token = make_token(&enc, "user-123", -3600);
        let app = init_app(dec, None).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(actix_http::StatusCode::UNAUTHORIZED, resp.status());
    }

    #[actix_web::test]
    async fn token_signed_with_other_key_is_rejected() {
        let (enc, _dec) = generate_keys();
        let (_other_enc, other_dec) = generate_keys();
        let token = make_token(&enc, "user-123", 3600);
        // App trusts `other_dec`, but the token was signed with `enc`.
        let app = init_app(other_dec, None).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(actix_http::StatusCode::UNAUTHORIZED, resp.status());
    }

    #[actix_web::test]
    async fn valid_session_cookie_is_authenticated() {
        let (enc, dec) = generate_keys();
        let token = make_token(&enc, "cookie-user", 3600);
        let app = init_app(dec, Some(token)).await;

        // Login stores the JWT in the session cookie.
        let login_resp =
            test::call_service(&app, test::TestRequest::get().uri("/login").to_request()).await;
        assert_eq!(actix_http::StatusCode::OK, login_resp.status());

        // Replay the session cookie against a protected route (no Authorization header).
        let mut req = test::TestRequest::get().uri("/protected");
        for cookie in login_resp.response().cookies() {
            req = req.cookie(cookie);
        }
        let resp = app.call(req.to_request()).await.unwrap();
        assert_eq!(actix_http::StatusCode::OK, resp.status());
        let claims: TestClaims = test::read_body_json(resp).await;
        assert_eq!("cookie-user", claims.sub);
    }

    /// Rejects every session of `revoked_subject`, whatever its issue time.
    struct RevokedSubjectChecker {
        revoked_subject: &'static str,
    }

    impl SessionTokenChecker for RevokedSubjectChecker {
        fn is_active(
            &self,
            subject: String,
            _issued_at: i64,
        ) -> LocalBoxFuture<'static, Result<bool, UniversalInboxError>> {
            let is_active = subject != self.revoked_subject;
            async move { Ok(is_active) }.boxed_local()
        }
    }

    fn settings_with_revoked_session(
        decoding_key: DecodingKey,
        revoked_subject: &'static str,
    ) -> AuthenticateMiddlewareSettings {
        AuthenticateMiddlewareSettings {
            session_token_checker: Some(Arc::new(RevokedSubjectChecker { revoked_subject })),
            ..settings(decoding_key)
        }
    }

    #[actix_web::test]
    async fn revoked_session_cookie_is_rejected() {
        let (enc, dec) = generate_keys();
        let token = make_token(&enc, "cookie-user", 3600);
        let app = init_app_with_settings(
            settings_with_revoked_session(dec, "cookie-user"),
            Some(token),
        )
        .await;

        let login_resp =
            test::call_service(&app, test::TestRequest::get().uri("/login").to_request()).await;
        let mut req = test::TestRequest::get().uri("/protected");
        for cookie in login_resp.response().cookies() {
            req = req.cookie(cookie);
        }
        let resp = app.call(req.to_request()).await.unwrap();
        assert_eq!(actix_http::StatusCode::UNAUTHORIZED, resp.status());
    }

    #[actix_web::test]
    async fn session_checker_does_not_apply_to_bearer_tokens() {
        let (enc, dec) = generate_keys();
        let token = make_token(&enc, "cookie-user", 3600);
        let app =
            init_app_with_settings(settings_with_revoked_session(dec, "cookie-user"), None).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(actix_http::StatusCode::OK, resp.status());
    }
}
