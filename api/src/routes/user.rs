use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_jwt_authc::Authenticated;
use actix_session::Session;
use actix_web::{HttpResponse, Scope, web};
use anyhow::{Context, anyhow};
use chrono::{TimeDelta, Utc};
use email_address::EmailAddress;
use redis::AsyncCommands;
use secrecy::{ExposeSecret, SecretBox};
use serde_json::json;
use tokio::sync::RwLock;
use validator::Validate;
use webauthn_rs::prelude::*;

use universal_inbox::{
    SuccessResponse,
    auth::auth_token::{AuthenticationToken, TruncatedAuthenticationToken},
    user::{
        Credentials, EmailValidationToken, Password, PasswordResetToken, RegisterUserParameters,
        User, UserId, UserPatch, Username,
    },
};

use crate::{
    configuration::Settings,
    routes::auth::USER_AUTH_KIND_SESSION_KEY,
    universal_inbox::{
        UniversalInboxError, UpdateStatus,
        auth_token::service::AuthenticationTokenService,
        user::{
            model::{LocalUserAuth, UserAuth, UserAuthKind},
            service::UserService,
        },
    },
    utils::{
        cache::Cache,
        jwt::{Claims, JWT_SESSION_KEY},
    },
};

const PASSKEY_REGISTRATION_STATE_SESSION_KEY: &str = "passkey-registration-state";
const PASSKEY_AUTHENTICATION_STATE_SESSION_KEY: &str = "passkey-authentication-state";

pub fn scope() -> Scope {
    web::scope("/users")
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
                        .route(web::post().to(login_user))
                        .route(web::patch().to(patch_user)),
                )
                .service(
                    web::resource("/email-verification")
                        .route(web::post().to(send_verification_email)),
                )
                .service(
                    web::resource("/authentication-tokens")
                        .route(web::get().to(list_authentication_tokens))
                        .route(web::post().to(create_authentication_token)),
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

    let updated_user = service
        .patch_user(&mut transaction, user_id, &patch.into_inner())
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while patching user")?;

    match updated_user {
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
        UpdateStatus {
            updated: _,
            result: None,
        } => Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(BoxBody::new(
                json!({ "message": format!("Cannot update unknown user {user_id}") }).to_string(),
            ))),
    }
}

pub async fn register_user(
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    settings: web::Data<Settings>,
    register_user_parameters: web::Json<RegisterUserParameters>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
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
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    credentials: web::Json<Credentials>,
    session: Session,
) -> Result<web::Json<User>, UniversalInboxError> {
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

    Ok(web::Json(user))
}

pub async fn send_verification_email(
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
    user_service: web::Data<Arc<UserService>>,
    path_info: web::Path<(UserId, EmailValidationToken)>,
) -> Result<HttpResponse, UniversalInboxError> {
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
    user_service: web::Data<Arc<UserService>>,
    email_address: web::Json<EmailAddress>,
) -> Result<HttpResponse, UniversalInboxError> {
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
    user_service: web::Data<Arc<UserService>>,
    path_info: web::Path<(UserId, PasswordResetToken)>,
    password: web::Json<SecretBox<Password>>,
) -> Result<HttpResponse, UniversalInboxError> {
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

#[allow(dependency_on_unit_never_type_fallback)]
pub async fn start_passkey_registration(
    user_service: web::Data<Arc<UserService>>,
    session: Session,
    cache: web::Data<Cache>,
    username: web::Json<Username>,
) -> Result<web::Json<CreationChallengeResponse>, UniversalInboxError> {
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

    session
        .insert(
            PASSKEY_REGISTRATION_STATE_SESSION_KEY,
            (username.0.as_str(), user_id),
        )
        .context("Failed to insert Passkey registration state into the session")?;
    let Ok(registration_state_to_store) = serde_json::to_string(&registration_state) else {
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

    Ok(web::Json(creation_challenge_response))
}

pub async fn finish_passkey_registration(
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    session: Session,
    cache: web::Data<Cache>,
    register_credentials: web::Json<RegisterPublicKeyCredential>,
) -> Result<web::Json<User>, UniversalInboxError> {
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while finishing Passkey registration")?;

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
    let Ok(registration_state) = serde_json::from_str(&str) else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Failed to parse Passkey registration state"
        )));
    };

    let new_user = service
        .finish_passkey_registration(
            &mut transaction,
            &username,
            user_id,
            register_credentials.into_inner(),
            registration_state,
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

    Ok(web::Json(new_user))
}

#[allow(dependency_on_unit_never_type_fallback)]
pub async fn start_passkey_authentication(
    user_service: web::Data<Arc<UserService>>,
    session: Session,
    cache: web::Data<Cache>,
    username: web::Json<Username>,
) -> Result<web::Json<RequestChallengeResponse>, UniversalInboxError> {
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

    session
        .insert(PASSKEY_AUTHENTICATION_STATE_SESSION_KEY, user_id)
        .context("Failed to insert Passkey authentication state into the session")?;
    let Ok(authentication_state_to_store) = serde_json::to_string(&authentication_state) else {
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

    Ok(web::Json(request_challenge_response))
}

pub async fn finish_passkey_authentication(
    user_service: web::Data<Arc<UserService>>,
    auth_token_service: web::Data<Arc<RwLock<AuthenticationTokenService>>>,
    session: Session,
    cache: web::Data<Cache>,
    credentials: web::Json<PublicKeyCredential>,
) -> Result<web::Json<User>, UniversalInboxError> {
    let service = user_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while finishing Passkey authentication")?;

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
    let Ok(authentication_state) = serde_json::from_str(&str) else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Failed to load Passkey authentication state"
        )));
    };

    let user = service
        .finish_passkey_authentication(
            &mut transaction,
            user_id,
            credentials.into_inner(),
            authentication_state,
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
        .context("Failed to commit while finishing Passkey authentication")?;

    Ok(web::Json(user))
}
