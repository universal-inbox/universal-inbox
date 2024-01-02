use std::sync::Arc;

use anyhow::Context;
use email_address::EmailAddress;
use log::{error, info};
use tokio::sync::RwLock;

use crate::universal_inbox::{user::service::UserService, UniversalInboxError};

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
