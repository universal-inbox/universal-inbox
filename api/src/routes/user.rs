use std::{num::NonZeroU32, sync::Arc};

use actix_http::body::BoxBody;
use actix_jwt_authc::Authenticated;
use actix_session::Session;
use actix_web::{HttpRequest, HttpResponse, Scope, web};
use anyhow::{Context, anyhow};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{TimeDelta, Utc};
use email_address::EmailAddress;
use governor::Quota;
use rand::RngExt;
use redis::AsyncCommands;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use serde_json::json;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use validator::Validate;
use webauthn_rs::prelude::*;

use universal_inbox::{
    SuccessResponse,
    auth::auth_token::{AuthenticationToken, TruncatedAuthenticationToken},
    user::{
        Credentials, EmailValidationToken, Password, PasswordResetToken, RegisterUserParameters,
        User, UserAuthKind, UserAuthMethod, UserId, UserPatch, UserPreferences,
        UserPreferencesPatch, Username,
    },
};

use crate::{
    configuration::Settings,
    routes::auth::USER_AUTH_KIND_SESSION_KEY,
    universal_inbox::{
        UniversalInboxError, UpdateStatus,
        auth_token::service::AuthenticationTokenService,
        oauth2::service::OAuth2Service,
        user::{
            model::{LocalUserAuth, UserAuth},
            service::UserService,
        },
    },
    utils::{
        cache::Cache,
        jwt::{Claims, JWT_SESSION_KEY},
        origin::check_request_origin,
        rate_limit::{IpRateLimiter, check_ip_rate_limit},
    },
};

const PASSKEY_REGISTRATION_STATE_SESSION_KEY: &str = "passkey-registration-state";
const PASSKEY_AUTHENTICATION_STATE_SESSION_KEY: &str = "passkey-authentication-state";

/// Length of the per-ceremony nonce in bytes. 16 random bytes (128 bits)
/// is well over the WebAuthn challenge entropy (16 bytes is the spec
/// minimum) and makes a guess by a network attacker infeasible.
const PASSKEY_NONCE_BYTES: usize = 16;

/// State blob persisted to Redis for a passkey ceremony, paired with a
/// per-ceremony nonce.
///
/// Each in-flight ceremony binds two independent values:
///
/// 1. a fresh 16-byte server-generated nonce returned in the start
///    response body and required (echoed) in the matching finish
///    request body;
/// 2. the same nonce embedded in this state blob.
///
/// The finish handler requires both to match in constant time. Without
/// the nonce echo a victim's cookie session by itself is no longer
/// sufficient to drive a finish call, closing the CSRF +
/// cross-flow-confusion gap flagged by DeepSec. Cross-flow consumption
/// is independently prevented by the disjoint Redis key namespaces
/// `add-passkey-registration-state::{user_id}`,
/// `passkey-registration-state::{user_id}`, and
/// `passkey-authentication-state::{user_id}` under which this struct is
/// stored.
#[derive(Serialize, Deserialize)]
struct NonceBound<T> {
    nonce: String,
    state: T,
}

/// Generate a fresh base64url-encoded nonce for one passkey ceremony.
fn generate_passkey_nonce() -> String {
    let mut bytes = [0u8; PASSKEY_NONCE_BYTES];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Wrapper request body for the four passkey **finish** handlers. The
/// client must echo the nonce returned by the matching start response;
/// the credential payload is flattened so the JSON shape remains
/// `{"nonce": "...", ...credential fields...}`.
#[derive(Deserialize)]
pub struct PasskeyFinishRequest<T> {
    nonce: String,
    #[serde(flatten)]
    credential: T,
}

/// Verify that `provided_nonce` (echoed by the client in the finish
/// request body) matches `expected_nonce` (the nonce embedded in the
/// Redis state blob loaded by the caller). Constant-time comparison.
///
/// Returns `Err(Unauthorized)` on mismatch with a generic envelope.
fn verify_passkey_nonce(
    expected_nonce: &str,
    provided_nonce: &str,
) -> Result<(), UniversalInboxError> {
    if !bool::from(expected_nonce.as_bytes().ct_eq(provided_nonce.as_bytes())) {
        return Err(UniversalInboxError::Unauthorized(anyhow!(
            "Passkey nonce mismatch"
        )));
    }
    Ok(())
}

/// Per-IP request budget for authentication-state-changing endpoints
/// (universal-inbox-bkj.30).
///
/// 30 req/min/IP is chosen to:
/// - Defeat password brute force (a real user typos a handful of times,
///   never approaches 30/min)
/// - Cap account-creation spam, email-verification floods, and password-
///   reset email-bombs at a rate that no humans hit
/// - Tolerate the worst-case interactive WebAuthn ceremony (start + finish
///   per attempt) repeated several times in a row
/// - Stay generous enough that the hermetic test suite, which exercises
///   register/login/passkey flows in rapid succession against `127.0.0.1`,
///   does not hit the limit
///
/// One shared limiter covers all auth endpoints rather than per-endpoint
/// limiters: an attacker pivoting from `login` to `password-reset` to
/// `email-verification` from a single IP should drain a shared bucket, not
/// reset their budget at each endpoint. Email- and username-based
/// secondary keying (so a single attacker rotating IPs cannot still
/// email-bomb one victim) is a follow-up.
const AUTH_RATE_LIMIT_PER_MINUTE: u32 = 30;

pub type AuthRateLimiter = IpRateLimiter;

pub fn build_auth_rate_limiter() -> Arc<AuthRateLimiter> {
    let quota = Quota::per_minute(
        NonZeroU32::new(AUTH_RATE_LIMIT_PER_MINUTE)
            .expect("AUTH_RATE_LIMIT_PER_MINUTE must be non-zero"),
    );
    Arc::new(AuthRateLimiter::keyed(quota))
}

pub fn scope(auth_rate_limiter: Arc<AuthRateLimiter>) -> Scope {
    web::scope("/users")
        .app_data(web::Data::new(auth_rate_limiter))
        .service(
            web::resource("")
                .name("users")
                .route(web::post().to(register_user)),
        )
        .service(web::resource("/password-reset").route(web::post().to(send_password_reset_email)))
        .service(
            web::scope("/me")
                .service(
                    web::resource("")
                        .route(web::get().to(get_user))
                        .route(web::patch().to(patch_user))
                        .route(web::post().to(login_user)),
                )
                .service(
                    web::resource("/email-verification")
                        .route(web::post().to(send_verification_email)),
                )
                .service(
                    web::scope("/auth-methods")
                        .service(web::resource("").route(web::get().to(list_auth_methods)))
                        .service(
                            web::resource("/local").route(web::post().to(add_local_auth_method)),
                        )
                        .service(
                            web::scope("/passkey/registration")
                                .service(
                                    web::resource("/start")
                                        .route(web::post().to(start_add_passkey_registration)),
                                )
                                .service(
                                    web::resource("/finish")
                                        .route(web::post().to(finish_add_passkey_registration)),
                                ),
                        )
                        .service(
                            web::resource("/{kind}").route(web::delete().to(remove_auth_method)),
                        ),
                )
                .service(
                    web::resource("/authentication-tokens")
                        .route(web::get().to(list_authentication_tokens))
                        .route(web::post().to(create_authentication_token)),
                )
                .service(
                    web::resource("/oauth2-authorized-clients")
                        .route(web::get().to(list_oauth2_authorized_clients)),
                )
                .service(
                    web::resource("/oauth2-authorized-clients/{client_id}")
                        .route(web::delete().to(revoke_oauth2_authorized_client)),
                )
                .service(
                    web::resource("/preferences")
                        .route(web::get().to(get_user_preferences))
                        .route(web::patch().to(patch_user_preferences)),
                ),
        )
        .service(
            web::resource("/{user_id}/email-verification/{email_validation_token}")
                .route(web::get().to(verify_email)),
        )
        .service(
            web::resource("/{user_id}/password-reset/{password_reset_token}")
                .route(web::post().to(reset_password)),
        )
        .service(
            web::scope("/passkeys")
                .service(
                    web::scope("/registration")
                        .service(
                            web::resource("/start")
                                .route(web::post().to(start_passkey_registration)),
                        )
                        .service(
                            web::resource("/finish")
                                .route(web::post().to(finish_passkey_registration)),
                        ),
                )
                .service(
                    web::scope("/authentication")
                        .service(
                            web::resource("/start")
                                .route(web::post().to(start_passkey_authentication)),
                        )
                        .service(
                            web::resource("/finish")
                                .route(web::post().to(finish_passkey_authentication)),
                        ),
                ),
        )
}

pub async fn get_user(
    user_service: web::Data<Arc<UserService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while fetching user")?;

    match service.get_user(&mut transaction, user_id).await? {
        Some(user) => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&user).context("Cannot serialize user")?)),
        None => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot find user {user_id}") }).to_string(),
            ))),
    }
}

pub async fn patch_user(
    user_service: web::Data<Arc<UserService>>,
    authenticated: Authenticated<Claims>,
    patch: web::Json<UserPatch>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while patching user")?;

    let update_status = service
        .patch_user(&mut transaction, user_id, &patch.into_inner())
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while patching user")?;

    match update_status {
        UpdateStatus {
            updated: true,
            result: Some(user),
        } => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(serde_json::to_string(&user).context("Cannot serialize user")?)),
        UpdateStatus {
            updated: false,
            result: Some(_),
        } => Ok(HttpResponse::NotModified().finish()),
        UpdateStatus { result: None, .. } => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot find user {user_id}") }).to_string(),
            ))),
    }
}

pub async fn list_auth_methods(
    user_service: web::Data<Arc<UserService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing auth methods")?;

    let auth_methods: Vec<UserAuthMethod> = service
        .list_user_auth_methods(&mut transaction, user_id)
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&auth_methods)
            .context("Cannot serialize auth methods list result")?,
    ))
}

const ADD_PASSKEY_REGISTRATION_STATE_SESSION_KEY: &str = "add-passkey-registration-state";

pub async fn add_local_auth_method(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    settings: web::Data<Settings>,
    authenticated: Authenticated<Claims>,
    password: web::Json<SecretBox<Password>>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_request_origin(&req, &settings.application.front_base_url) {
        return Ok(response);
    }
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while adding local auth method")?;

    let auth_method = service
        .add_local_auth_method(&mut transaction, user_id, password.into_inner())
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while adding local auth method")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&auth_method).context("Cannot serialize auth method")?))
}

#[allow(clippy::too_many_arguments, dependency_on_unit_never_type_fallback)]
pub async fn start_add_passkey_registration(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    settings: web::Data<Settings>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    authenticated: Authenticated<Claims>,
    session: Session,
    cache: web::Data<Cache>,
    username: web::Json<Username>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_request_origin(&req, &settings.application.front_base_url) {
        return Ok(response);
    }
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while starting add Passkey registration")?;

    session.remove(ADD_PASSKEY_REGISTRATION_STATE_SESSION_KEY);

    let username = username.into_inner();
    let (creation_challenge_response, registration_state) = service
        .start_add_passkey_auth_method(&mut transaction, user_id, &username)
        .await?;

    let nonce = generate_passkey_nonce();
    session
        .insert(
            ADD_PASSKEY_REGISTRATION_STATE_SESSION_KEY,
            (username.0.as_str(), user_id),
        )
        .context("Failed to insert add Passkey registration state into the session")?;
    let bound = NonceBound {
        nonce: nonce.clone(),
        state: registration_state,
    };
    let Ok(registration_state_to_store) = serde_json::to_string(&bound) else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Failed to serialize add Passkey registration state"
        )));
    };
    cache
        .connection_manager
        .clone()
        .set::<_, _, ()>(
            format!(
                "{}::{}",
                ADD_PASSKEY_REGISTRATION_STATE_SESSION_KEY, user_id
            ),
            registration_state_to_store,
        )
        .await
        .context("Failed to store add Passkey registration state in Redis")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while starting add Passkey registration")?;

    // The challenge response is serialized as a JSON object; splicing the
    // server-generated `nonce` into that object keeps the response shape
    // backwards-compatible for any field clients already read while adding
    // the required echo-back field.
    let mut response_value = serde_json::to_value(&creation_challenge_response)
        .context("Cannot serialize Passkey creation challenge")?;
    if let Some(obj) = response_value.as_object_mut() {
        obj.insert("nonce".to_string(), serde_json::Value::String(nonce));
    }
    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&response_value)
            .context("Cannot serialize Passkey creation challenge with nonce")?,
    ))
}

#[allow(clippy::too_many_arguments)]
pub async fn finish_add_passkey_registration(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    settings: web::Data<Settings>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    authenticated: Authenticated<Claims>,
    session: Session,
    cache: web::Data<Cache>,
    body: web::Json<PasskeyFinishRequest<RegisterPublicKeyCredential>>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_request_origin(&req, &settings.application.front_base_url) {
        return Ok(response);
    }
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while finishing add Passkey registration")?;

    let PasskeyFinishRequest {
        nonce: provided_nonce,
        credential: register_credentials,
    } = body.into_inner();

    let (username, session_user_id): (String, UserId) = session
        .get(ADD_PASSKEY_REGISTRATION_STATE_SESSION_KEY)
        .context("Failed to extract add Passkey registration state from the session")?
        .ok_or_else(|| anyhow!("Unable to find add Passkey registration state in session"))?;

    if session_user_id != user_id {
        return Err(UniversalInboxError::Unauthorized(anyhow!(
            "Session user ID does not match authenticated user ID"
        )));
    }

    session.remove(ADD_PASSKEY_REGISTRATION_STATE_SESSION_KEY);
    let str: String = cache
        .connection_manager
        .clone()
        .get_del(format!(
            "{}::{}",
            ADD_PASSKEY_REGISTRATION_STATE_SESSION_KEY, user_id
        ))
        .await
        .context("Failed to fetch add Passkey registration state from Redis")?;
    let Ok(bound) = serde_json::from_str::<NonceBound<PasskeyRegistration>>(&str) else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Failed to parse add Passkey registration state"
        )));
    };

    verify_passkey_nonce(&bound.nonce, &provided_nonce)?;

    let username = Username(username);
    let auth_method = service
        .finish_add_passkey_auth_method(
            &mut transaction,
            &username,
            user_id,
            register_credentials,
            bound.state,
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while finishing add Passkey registration")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&auth_method).context("Cannot serialize auth method")?))
}

pub async fn remove_auth_method(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    settings: web::Data<Settings>,
    authenticated: Authenticated<Claims>,
    path_info: web::Path<UserAuthKind>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_request_origin(&req, &settings.application.front_base_url) {
        return Ok(response);
    }
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let kind = path_info.into_inner();
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while removing auth method")?;

    service
        .remove_auth_method(&mut transaction, user_id, kind)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while removing auth method")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&SuccessResponse {
            success: true,
            message: format!("Authentication method {kind} successfully removed"),
        })
        .context("Cannot serialize response")?,
    ))
}

pub async fn register_user(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    settings: web::Data<Settings>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    register_user_parameters: web::Json<RegisterUserParameters>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let user_service = user_service.clone();
    let mut transaction = user_service
        .begin()
        .await
        .context("Failed to create new transaction while registering user")?;

    register_user_parameters
        .validate()
        .map_err(UniversalInboxError::InvalidParameters)?;

    let email_domain = register_user_parameters
        .credentials
        .email
        .domain()
        .to_lowercase();

    if let Some(rejection_message) = settings
        .application
        .security
        .email_domain_blacklist
        .get(&email_domain)
    {
        return Err(UniversalInboxError::Forbidden(rejection_message.clone()));
    }

    let user = user_service
        .register_user(
            &mut transaction,
            User::new(
                None,
                None,
                register_user_parameters.credentials.email.clone(),
            ),
            UserAuth::Local(Box::new(LocalUserAuth {
                password_hash: user_service
                    .get_new_password_hash(register_user_parameters.credentials.password.clone())?,
                password_reset_at: None,
                password_reset_sent_at: None,
            })),
        )
        .await
        .map_err(|err| {
            if let UniversalInboxError::AlreadyExists { .. } = err {
                UniversalInboxError::Unauthorized(anyhow!(
                    "A user with this email address already exists"
                ))
            } else {
                err
            }
        })?;

    let auth_token_service = auth_token_service.read().await;

    let auth_token = auth_token_service
        .create_auth_token(&mut transaction, true, user.id, None, false)
        .await?;
    session
        .insert(
            JWT_SESSION_KEY,
            auth_token.jwt_token.expose_secret().0.clone(),
        )
        .context("Failed to insert JWT token into the session")?;
    session
        .insert(USER_AUTH_KIND_SESSION_KEY, UserAuthKind::Local)
        .context("Failed to insert authentication type into the session")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while registering user")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&user).context("Cannot serialize user")?))
}

pub async fn login_user(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    credentials: web::Json<Credentials>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while logging in user")?;

    let user = service
        .validate_credentials(&mut transaction, credentials.into_inner())
        .await
        .map_err(|err| {
            if let UniversalInboxError::Unauthorized(_) = err {
                UniversalInboxError::Unauthorized(anyhow!("Invalid email address or password"))
            } else {
                err
            }
        })?;

    let auth_token_service = auth_token_service.read().await;

    let auth_token = auth_token_service
        .create_auth_token(&mut transaction, true, user.id, None, false)
        .await?;
    session
        .insert(
            JWT_SESSION_KEY,
            auth_token.jwt_token.expose_secret().0.clone(),
        )
        .context("Failed to insert JWT token into the session")?;
    session
        .insert(USER_AUTH_KIND_SESSION_KEY, UserAuthKind::Local)
        .context("Failed to insert authentication type into the session")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while logging in user")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&user).context("Cannot serialize user")?))
}

pub async fn send_verification_email(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while sending verification email")?;

    service
        .send_verification_email(&mut transaction, user_id, false)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while sending verification email")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&SuccessResponse {
            success: true,
            message: "Email verification successfully sent".to_string(),
        })
        .context("Cannot serialize response")?,
    ))
}

pub async fn verify_email(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    path_info: web::Path<(UserId, EmailValidationToken)>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let (user_id, email_validation_token) = path_info.into_inner();
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while verifying email validation token")?;

    service
        .verify_email(&mut transaction, user_id, email_validation_token)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while verifying email validation token")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&SuccessResponse {
            success: true,
            message: "Email successfully verified".to_string(),
        })
        .context("Cannot serialize response")?,
    ))
}

pub async fn send_password_reset_email(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    email_address: web::Json<EmailAddress>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while sending password reset email")?;

    service
        .send_password_reset_email(&mut transaction, email_address.into_inner(), false)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while sending password reset email")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&SuccessResponse {
            success: true,
            message: "Reset password email successfully sent".to_string(),
        })
        .context("Cannot serialize response")?,
    ))
}

pub async fn reset_password(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    path_info: web::Path<(UserId, PasswordResetToken)>,
    password: web::Json<SecretBox<Password>>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let (user_id, password_reset_token) = path_info.into_inner();
    let service = user_service.clone();
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while resetting the password of {user_id}"
    ))?;

    service
        .reset_password(
            &mut transaction,
            user_id,
            password_reset_token,
            password.into_inner(),
        )
        .await?;

    transaction.commit().await.context(format!(
        "Failed to commit while resetting the password of {user_id}"
    ))?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&SuccessResponse {
            success: true,
            message: "Password successfully reset".to_string(),
        })
        .context("Cannot serialize response")?,
    ))
}

pub async fn list_authentication_tokens(
    authentication_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = authentication_token_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing authentication tokens")?;
    let result: Vec<TruncatedAuthenticationToken> = service
        .fetch_auth_tokens_for_user(&mut transaction, user_id)
        .await?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&result)
            .context("Cannot serialize authentication tokens list result")?,
    ))
}

pub async fn create_authentication_token(
    authentication_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = authentication_token_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating authentication token")?;
    let result: AuthenticationToken = service
        .create_auth_token(
            &mut transaction,
            false,
            user_id,
            Some(Utc::now() + TimeDelta::try_days(30 * 6).unwrap()),
            true,
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while creating authentication token")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&result).context("Cannot serialize created authentication token")?,
    ))
}

pub async fn get_user_preferences(
    user_service: web::Data<Arc<UserService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while fetching user preferences")?;

    let preferences = service
        .get_user_preferences(&mut transaction, user_id)
        .await?;

    match preferences {
        Some(prefs) => Ok(HttpResponse::Ok().json(prefs)),
        None => Ok(HttpResponse::Ok().json(UserPreferences {
            user_id,
            default_task_manager_provider_kind: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })),
    }
}

pub async fn patch_user_preferences(
    user_service: web::Data<Arc<UserService>>,
    patch: web::Json<UserPreferencesPatch>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while patching user preferences")?;

    let preferences = service
        .patch_user_preferences(&mut transaction, user_id, &patch)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while patching user preferences")?;

    Ok(HttpResponse::Ok().json(preferences))
}

#[allow(dependency_on_unit_never_type_fallback)]
pub async fn start_passkey_registration(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    settings: web::Data<Settings>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    session: Session,
    cache: web::Data<Cache>,
    username: web::Json<Username>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_request_origin(&req, &settings.application.front_base_url) {
        return Ok(response);
    }
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while starting Passkey registration")?;

    session.remove(PASSKEY_REGISTRATION_STATE_SESSION_KEY);

    let username = username.into_inner();
    let (user_id, creation_challenge_response, registration_state) = service
        .start_passkey_registration(&mut transaction, &username)
        .await?;

    let nonce = generate_passkey_nonce();
    session
        .insert(
            PASSKEY_REGISTRATION_STATE_SESSION_KEY,
            (username.0.as_str(), user_id),
        )
        .context("Failed to insert Passkey registration state into the session")?;
    let bound = NonceBound {
        nonce: nonce.clone(),
        state: registration_state,
    };
    let Ok(registration_state_to_store) = serde_json::to_string(&bound) else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Failed to serialize Passkey registration state"
        )));
    };
    cache
        .connection_manager
        .clone()
        .set::<_, _, ()>(
            format!("{}::{}", PASSKEY_REGISTRATION_STATE_SESSION_KEY, user_id),
            registration_state_to_store,
        )
        .await
        .context("Failed to store Passkey registration state in Redis")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while starting Passkey registration")?;

    let mut response_value = serde_json::to_value(&creation_challenge_response)
        .context("Cannot serialize Passkey creation challenge")?;
    if let Some(obj) = response_value.as_object_mut() {
        obj.insert("nonce".to_string(), serde_json::Value::String(nonce));
    }
    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&response_value)
            .context("Cannot serialize Passkey creation challenge with nonce")?,
    ))
}

#[allow(clippy::too_many_arguments)]
pub async fn finish_passkey_registration(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    settings: web::Data<Settings>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    session: Session,
    cache: web::Data<Cache>,
    body: web::Json<PasskeyFinishRequest<RegisterPublicKeyCredential>>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_request_origin(&req, &settings.application.front_base_url) {
        return Ok(response);
    }
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while finishing Passkey registration")?;

    let PasskeyFinishRequest {
        nonce: provided_nonce,
        credential: register_credentials,
    } = body.into_inner();

    let (username, user_id) = session
        .get(PASSKEY_REGISTRATION_STATE_SESSION_KEY)
        .context("Failed to extract Passkey registration state from the session")?
        .ok_or_else(|| anyhow!("Unable to find Passkey registration state in session"))?;
    session.remove(PASSKEY_REGISTRATION_STATE_SESSION_KEY);
    let str: String = cache
        .connection_manager
        .clone()
        .get_del(format!(
            "{}::{}",
            PASSKEY_REGISTRATION_STATE_SESSION_KEY, user_id
        ))
        .await
        .context("Failed to fetch Passkey registration state from Redis")?;
    let Ok(bound) = serde_json::from_str::<NonceBound<PasskeyRegistration>>(&str) else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Failed to parse Passkey registration state"
        )));
    };

    verify_passkey_nonce(&bound.nonce, &provided_nonce)?;

    let new_user = service
        .finish_passkey_registration(
            &mut transaction,
            &username,
            user_id,
            register_credentials,
            bound.state,
        )
        .await?;

    let auth_token_service = auth_token_service.read().await;
    let auth_token = auth_token_service
        .create_auth_token(&mut transaction, true, user_id, None, false)
        .await?;
    session
        .insert(
            JWT_SESSION_KEY,
            auth_token.jwt_token.expose_secret().0.clone(),
        )
        .context("Failed to insert JWT token into the session")?;
    session
        .insert(USER_AUTH_KIND_SESSION_KEY, UserAuthKind::Passkey)
        .context("Failed to insert authentication type into the session")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while finishing Passkey registration")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&new_user).context("Cannot serialize user")?))
}

#[allow(dependency_on_unit_never_type_fallback)]
pub async fn start_passkey_authentication(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    settings: web::Data<Settings>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    session: Session,
    cache: web::Data<Cache>,
    username: web::Json<Username>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_request_origin(&req, &settings.application.front_base_url) {
        return Ok(response);
    }
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while starting Passkey authentication")?;

    session.remove(PASSKEY_AUTHENTICATION_STATE_SESSION_KEY);

    let username = username.into_inner();
    let (user_id, request_challenge_response, authentication_state) = service
        .start_passkey_authentication(&mut transaction, &username)
        .await?;

    let nonce = generate_passkey_nonce();
    session
        .insert(PASSKEY_AUTHENTICATION_STATE_SESSION_KEY, user_id)
        .context("Failed to insert Passkey authentication state into the session")?;
    let bound = NonceBound {
        nonce: nonce.clone(),
        state: authentication_state,
    };
    let Ok(authentication_state_to_store) = serde_json::to_string(&bound) else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Failed to serialize Passkey authentication state"
        )));
    };
    cache
        .connection_manager
        .clone()
        .set::<_, _, ()>(
            format!("{}::{}", PASSKEY_AUTHENTICATION_STATE_SESSION_KEY, user_id),
            authentication_state_to_store,
        )
        .await
        .context("Failed to store Passkey authentication state in Redis")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while starting Passkey authentication")?;

    let mut response_value = serde_json::to_value(&request_challenge_response)
        .context("Cannot serialize Passkey request challenge")?;
    if let Some(obj) = response_value.as_object_mut() {
        obj.insert("nonce".to_string(), serde_json::Value::String(nonce));
    }
    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&response_value)
            .context("Cannot serialize Passkey request challenge with nonce")?,
    ))
}

pub async fn list_oauth2_authorized_clients(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing OAuth2 authorized clients")?;

    let clients = service
        .list_authorized_clients(&mut transaction, user_id)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while listing OAuth2 authorized clients")?;

    Ok(HttpResponse::Ok().content_type("application/json").body(
        serde_json::to_string(&clients)
            .context("Cannot serialize OAuth2 authorized clients list")?,
    ))
}

pub async fn revoke_oauth2_authorized_client(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    authenticated: Authenticated<Claims>,
    path: web::Path<String>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let client_id = path.into_inner();
    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while revoking OAuth2 client authorization")?;

    service
        .revoke_client_authorization(&mut transaction, user_id, &client_id)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while revoking OAuth2 client authorization")?;

    Ok(HttpResponse::Ok().json(universal_inbox::SuccessResponse {
        success: true,
        message: "OAuth2 client authorization revoked".to_string(),
    }))
}

#[allow(clippy::too_many_arguments)]
pub async fn finish_passkey_authentication(
    req: HttpRequest,
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    settings: web::Data<Settings>,
    rate_limiter: web::Data<Arc<AuthRateLimiter>>,
    session: Session,
    cache: web::Data<Cache>,
    body: web::Json<PasskeyFinishRequest<PublicKeyCredential>>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Err(response) = check_request_origin(&req, &settings.application.front_base_url) {
        return Ok(response);
    }
    if let Err(response) = check_ip_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while finishing Passkey authentication")?;

    let PasskeyFinishRequest {
        nonce: provided_nonce,
        credential: credentials,
    } = body.into_inner();

    let user_id = session
        .get(PASSKEY_AUTHENTICATION_STATE_SESSION_KEY)
        .context("Failed to extract Passkey authentication state from the session")?
        .ok_or_else(|| anyhow!("Unable to find Passkey authentication state in session"))?;
    session.remove(PASSKEY_AUTHENTICATION_STATE_SESSION_KEY);
    let str: String = cache
        .connection_manager
        .clone()
        .get_del(format!(
            "{}::{}",
            PASSKEY_AUTHENTICATION_STATE_SESSION_KEY, user_id
        ))
        .await
        .context("Failed to fetch Passkey authentication state in Redis")?;
    let Ok(bound) = serde_json::from_str::<NonceBound<PasskeyAuthentication>>(&str) else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Failed to load Passkey authentication state"
        )));
    };

    verify_passkey_nonce(&bound.nonce, &provided_nonce)?;

    let user = service
        .finish_passkey_authentication(&mut transaction, user_id, credentials, bound.state)
        .await?;

    let auth_token_service = auth_token_service.read().await;
    let auth_token = auth_token_service
        .create_auth_token(&mut transaction, true, user_id, None, false)
        .await?;
    session
        .insert(
            JWT_SESSION_KEY,
            auth_token.jwt_token.expose_secret().0.clone(),
        )
        .context("Failed to insert JWT token into the session")?;
    session
        .insert(USER_AUTH_KIND_SESSION_KEY, UserAuthKind::Passkey)
        .context("Failed to insert authentication type into the session")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while finishing Passkey authentication")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&user).context("Cannot serialize user")?))
}
