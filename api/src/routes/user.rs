use std::sync::Arc;

use actix_http::body::BoxBody;
use actix_jwt_authc::{Authenticated, JWTSessionKey};
use actix_session::Session;
use actix_web::{web, HttpResponse, Scope};
use anyhow::{anyhow, Context};
use email_address::EmailAddress;
use jsonwebtoken::EncodingKey;
use secrecy::Secret;
use serde_json::json;
use tokio::sync::RwLock;
use validator::Validate;

use universal_inbox::{
    user::{
        Credentials, EmailValidationToken, LocalUserAuth, Password, PasswordResetToken,
        RegisterUserParameters, User, UserAuth, UserId,
    },
    SuccessResponse,
};

use crate::{
    universal_inbox::{user::service::UserService, UniversalInboxError},
    utils::jwt::{Claims, JWTttl},
};

pub fn scope() -> Scope {
    web::scope("/users")
        .service(
            web::resource("")
                .name("users")
                .route(web::post().to(register_user)),
        )
        .service(web::resource("/password_reset").route(web::post().to(send_password_reset_email)))
        .service(
            web::resource("me")
                .route(web::get().to(get_user))
                .route(web::post().to(login_user)),
        )
        .service(
            web::resource("/me/email_verification").route(web::post().to(send_verification_email)),
        )
        .service(
            web::resource("/{user_id}/email_verification/{email_validation_token}")
                .route(web::get().to(verify_email)),
        )
        .service(
            web::resource("/{user_id}/password_reset/{password_reset_token}")
                .route(web::post().to(reset_password)),
        )
}

pub async fn get_user(
    user_service: web::Data<Arc<RwLock<UserService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.read().await;
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

pub async fn register_user(
    user_service: web::Data<Arc<RwLock<UserService>>>,
    register_user_parameters: web::Json<RegisterUserParameters>,
    jwt_encoding_key: web::Data<EncodingKey>,
    jwt_session_key: web::Data<JWTSessionKey>,
    ttl: web::Data<JWTttl>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = user_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while registering user")?;

    register_user_parameters
        .validate()
        .map_err(UniversalInboxError::InvalidParameters)?;

    let user = service
        .register_user(
            &mut transaction,
            User::new(
                register_user_parameters.first_name.clone(),
                register_user_parameters.last_name.clone(),
                register_user_parameters.credentials.email.clone(),
                UserAuth::Local(LocalUserAuth {
                    password_hash: service.get_new_password_hash(
                        register_user_parameters.credentials.password.clone(),
                    )?,
                    password_reset_at: None,
                    password_reset_sent_at: None,
                }),
            ),
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

    let jwt_token = Claims::new_jwt_token(
        user.id.to_string(),
        &ttl.into_inner(),
        &jwt_encoding_key.into_inner(),
    )?;
    session
        .insert(&jwt_session_key.0, jwt_token)
        .context("Failed to insert JWT token into the session")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while registering user")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&user).context("Cannot serialize user")?))
}

pub async fn login_user(
    user_service: web::Data<Arc<RwLock<UserService>>>,
    credentials: web::Json<Credentials>,
    jwt_encoding_key: web::Data<EncodingKey>,
    jwt_session_key: web::Data<JWTSessionKey>,
    ttl: web::Data<JWTttl>,
    session: Session,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = user_service.read().await;
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

    let jwt_token = Claims::new_jwt_token(
        user.id.to_string(),
        &ttl.into_inner(),
        &jwt_encoding_key.into_inner(),
    )?;
    session
        .insert(&jwt_session_key.0, jwt_token)
        .context("Failed to insert JWT token into the session")?;

    transaction
        .commit()
        .await
        .context("Failed to commit while logging in user")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&user).context("Cannot serialize user")?))
}

pub async fn send_verification_email(
    user_service: web::Data<Arc<RwLock<UserService>>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let service = user_service.read().await;
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
    user_service: web::Data<Arc<RwLock<UserService>>>,
    path_info: web::Path<(UserId, EmailValidationToken)>,
) -> Result<HttpResponse, UniversalInboxError> {
    let (user_id, email_validation_token) = path_info.into_inner();
    let service = user_service.read().await;
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
    user_service: web::Data<Arc<RwLock<UserService>>>,
    email_address: web::Json<EmailAddress>,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = user_service.read().await;
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
    user_service: web::Data<Arc<RwLock<UserService>>>,
    path_info: web::Path<(UserId, PasswordResetToken)>,
    password: web::Json<Secret<Password>>,
) -> Result<HttpResponse, UniversalInboxError> {
    let (user_id, password_reset_token) = path_info.into_inner();
    let service = user_service.read().await;
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
