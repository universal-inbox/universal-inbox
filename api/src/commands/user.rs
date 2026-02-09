use std::sync::Arc;

use anyhow::Context;
use chrono::{TimeDelta, Utc};
use email_address::EmailAddress;
use log::{error, info};
use secrecy::ExposeSecret;
use tabled::{
    builder::Builder,
    settings::{Color, object::Rows, style::Style},
};
use tokio::sync::RwLock;

use universal_inbox::user::UserId;

use crate::universal_inbox::{
    UniversalInboxError,
    auth_token::service::AuthenticationTokenService,
    user::{model::UserAuth, service::UserService},
};

#[tracing::instrument(
    name = "send-verification-email-command",
    level = "info",
    skip(user_service),
    err
)]
pub async fn send_verification_email(
    user_service: Arc<UserService>,
    user_email: &EmailAddress,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    info!("Sending email verification to {user_email}");
    let service = user_service.clone();

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
    user_service: Arc<UserService>,
    user_email: &EmailAddress,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    info!("Sending the password reset email to {user_email}");
    let service = user_service.clone();

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
    skip(user_service, auth_token_service),
    err
)]
pub async fn generate_jwt_token(
    user_service: Arc<UserService>,
    auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
    user_email: &EmailAddress,
) -> Result<(), UniversalInboxError> {
    let service = user_service.clone();

    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while generating new authentication token for {user_email}"
    ))?;

    let user = service
        .get_user_by_email(&mut transaction, user_email)
        .await?
        .context(format!(
            "Unable to find user with email address {user_email}"
        ))?;

    let auth_token_service = auth_token_service.read().await;

    let auth_token = auth_token_service
        .create_auth_token(
            &mut transaction,
            false,
            user.id,
            Some(Utc::now() + TimeDelta::try_days(30 * 6).unwrap()),
            true,
        )
        .await?;

    transaction.commit().await.context(format!(
        "Failed to commit transaction while generating new authentication token for {user_email}"
    ))?;

    info!(
        "New JWT token for user {}: {}",
        user.id,
        auth_token.jwt_token.expose_secret().0
    );

    Ok(())
}

#[tracing::instrument(name = "list-users", level = "info", skip(user_service), err)]
pub async fn list_users(user_service: Arc<UserService>) -> Result<(), UniversalInboxError> {
    let service = user_service.clone();

    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while listing users")?;

    let users = service.fetch_all_users_and_auth(&mut transaction).await?;

    let mut rows: Vec<Vec<String>> = users
        .iter()
        .map(|(user, user_auth)| {
            vec![
                user.id.to_string(),
                user.email
                    .as_ref()
                    .map(|email| email.to_string())
                    .unwrap_or_default(),
                match user_auth {
                    UserAuth::Passkey(passkey_user_auth) => passkey_user_auth.username.to_string(),
                    _ => "".to_string(),
                },
                user_auth.to_string(),
            ]
        })
        .collect();
    rows.insert(
        0,
        vec![
            "User ID".to_string(),
            "Email".to_string(),
            "Username".to_string(),
            "Authentication".to_string(),
        ],
    );
    let mut user_table = Builder::from(rows).build();
    user_table
        .with(Style::rounded())
        .modify(Rows::first(), Color::FG_BLUE);

    println!("{}", user_table);

    Ok(())
}

#[tracing::instrument(name = "delete-user", level = "info", skip(user_service), err)]
pub async fn delete_user(
    user_service: Arc<UserService>,
    user_id: UserId,
) -> Result<(), UniversalInboxError> {
    let service = user_service.clone();

    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while deleting user {user_id}"
    ))?;

    service.delete_user(&mut transaction, user_id).await?;

    transaction.commit().await.context(format!(
        "Failed to commit transaction while deleting user {user_id}"
    ))?;

    info!("User {user_id} and its data was successfully deleted");

    Ok(())
}
