use std::sync::Arc;

use anyhow::Context;
use email_address::EmailAddress;
use log::{error, info};
use tokio::sync::RwLock;

use crate::{
    configuration::HttpSessionSettings,
    universal_inbox::{user::service::UserService, UniversalInboxError},
    utils::jwt::{Claims, JWTBase64EncodedSigningKeys, JWTSigningKeys, JWTttl},
};

#[tracing::instrument(
    name = "send-verification-email-command",
    level = "info",
    skip(user_service),
    err
)]
pub async fn send_verification_email(
    user_service: Arc<RwLock<UserService>>,
    user_email: &EmailAddress,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    info!("Sending email verification to {user_email}");
    let service = user_service.read().await;

    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while sending verification email to {user_email}"
    ))?;
    let user = service
        .get_user_by_email(&mut transaction, user_email)
        .await?
        .context(format!(
            "Unable to find user with email address {user_email}"
        ))?;

    let result = service
        .send_verification_email(&mut transaction, user.id, dry_run)
        .await;

    match result {
        Ok(_) => {
            if dry_run {
                transaction.rollback().await.context(
                    "Failed to rollback (dry-run) transaction while sending verification email",
                )?;
            } else {
                transaction
                    .commit()
                    .await
                    .context("Failed to commit transaction while sending verification email")?;
            }
            Ok(())
        }
        Err(err) => {
            error!("Failed to send email verification to {user_email}");
            transaction
                .rollback()
                .await
                .context("Failed to rollback transaction while sending verification email")?;
            Err(err)
        }
    }
}

#[tracing::instrument(
    name = "send-password-reset-email-command",
    level = "info",
    skip(user_service),
    err
)]
pub async fn send_password_reset_email(
    user_service: Arc<RwLock<UserService>>,
    user_email: &EmailAddress,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    info!("Sending the password reset email to {user_email}");
    let service = user_service.read().await;

    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while send the password reset email for {user_email}"
    ))?;

    let result = service
        .send_password_reset_email(&mut transaction, user_email.clone(), dry_run)
        .await;

    match result {
        Ok(_) => {
            if dry_run {
                transaction.rollback().await.context(
                    format!("Failed to rollback (dry-run) transaction while send the password reset email for {user_email}")
                )?;
            } else {
                transaction.commit().await.context(format!(
                    "Failed to commit transaction while send the password reset email for {user_email}"
                ))?;
            }
            Ok(())
        }
        Err(err) => {
            error!("Failed to send the password reset email for {user_email}");
            transaction.rollback().await.context(format!(
                "Failed to rollback transaction while send the password reset email for {user_email}"
            ))?;
            Err(err)
        }
    }
}

#[tracing::instrument(
    name = "generate-jwt-token",
    level = "info",
    skip(user_service, settings),
    err
)]
pub async fn generate_jwt_token(
    user_service: Arc<RwLock<UserService>>,
    settings: HttpSessionSettings,
    user_email: &EmailAddress,
) -> Result<(), UniversalInboxError> {
    let service = user_service.read().await;

    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while send the password reset email for {user_email}"
    ))?;

    let user = service
        .get_user_by_email(&mut transaction, user_email)
        .await?
        .context(format!(
            "Unable to find user with email address {user_email}"
        ))?;

    let jwt_signing_keys =
        JWTSigningKeys::load_from_base64_encoded_keys(JWTBase64EncodedSigningKeys {
            secret_key: settings.jwt_secret_key.clone(),
            public_key: settings.jwt_public_key.clone(),
        })?;
    let jwt_token = Claims::new_jwt_token(
        user.id.to_string(),
        &JWTttl(settings.jwt_token_expiration_in_days),
        &jwt_signing_keys.encoding_key,
    )?;

    info!("New JWT token for user {}: {}", user.id, jwt_token);

    Ok(())
}
