use std::{
    io::{self, BufRead, IsTerminal},
    sync::Arc,
};

use anyhow::Context;
use chrono::{TimeDelta, Utc};
use email_address::EmailAddress;
use log::{error, info};
use secrecy::{ExposeSecret, SecretBox};
use tabled::{
    builder::Builder,
    settings::{Color, object::Rows, style::Style},
};
use tokio::sync::RwLock;

use universal_inbox::user::{Password, UserAuthKind, UserId};

use crate::observability::RecordSpanError;
use crate::observability::attr;
use crate::{
    billing::service::BillingService,
    universal_inbox::{
        UniversalInboxError,
        auth_token::service::AuthenticationTokenService,
        user::{model::UserAuth, service::UserService},
    },
};

#[tracing::instrument(
    name = "send-verification-email-command",
    level = "info",
    skip(user_service, user_email),
    fields({ attr::ERROR_TYPE } = tracing::field::Empty)
)]
pub async fn send_verification_email(
    user_service: Arc<UserService>,
    user_email: &EmailAddress,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> =
        async move {
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
                        transaction.commit().await.context(
                            "Failed to commit transaction while sending verification email",
                        )?;
                    }
                    Ok(())
                }
                Err(err) => {
                    error!("Failed to send email verification to {user_email}");
                    transaction.rollback().await.context(
                        "Failed to rollback transaction while sending verification email",
                    )?;
                    Err(err)
                }
            }
        }
        .await;
    result.record_span_error()
}

#[tracing::instrument(
    name = "send-password-reset-email-command",
    level = "info",
    skip(user_service, user_email),
    fields({ attr::ERROR_TYPE } = tracing::field::Empty)
)]
pub async fn send_password_reset_email(
    user_service: Arc<UserService>,
    user_email: &EmailAddress,
    dry_run: bool,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async move {
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
}.await;
    result.record_span_error()
}

#[tracing::instrument(
    name = "generate-jwt-token",
    level = "info",
    skip(user_service, auth_token_service, user_email),
    fields({ attr::ERROR_TYPE } = tracing::field::Empty)
)]
pub async fn generate_jwt_token(
    user_service: Arc<UserService>,
    auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
    user_email: &EmailAddress,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async move {
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

    // The token is a live bearer credential: never send it through
    // `tracing`, whose subscribers ship log lines to stdout logging and the
    // OTLP exporter. Print it once, on stdout only, for the operator to copy.
    // Only its id goes to the logs; revoke it with
    // `DELETE /api/users/me/authentication-tokens/{id}` if it leaks.
    info!(
        "New API token {} generated for user {} (expires {:?})",
        auth_token.id, user.id, auth_token.expire_at
    );
    println!("{}", auth_token.jwt_token.expose_secret().0);

    Ok(())
}.await;
    result.record_span_error()
}

#[tracing::instrument(name = "list-users", level = "info", skip(user_service), fields({ attr::ERROR_TYPE } = tracing::field::Empty))]
pub async fn list_users(user_service: Arc<UserService>) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async move {
        let service = user_service.clone();

        let mut transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while listing users")?;

        let users = service.fetch_all_users_and_auth(&mut transaction).await?;

        let mut rows: Vec<Vec<String>> = users
            .iter()
            .map(|(user, user_auths)| {
                let usernames: Vec<String> = user_auths
                    .iter()
                    .filter_map(|ua| match ua {
                        UserAuth::Passkey(passkey_user_auth) => Some(sanitize_for_terminal(
                            &passkey_user_auth.username.to_string(),
                        )),
                        _ => None,
                    })
                    .collect();
                let auth_kinds: Vec<String> = user_auths.iter().map(|ua| ua.to_string()).collect();
                vec![
                    user.id.to_string(),
                    user.email
                        .as_ref()
                        .map(|email| sanitize_for_terminal(email.as_ref()))
                        .unwrap_or_default(),
                    usernames.join(", "),
                    auth_kinds.join(", "),
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
    .await;
    result.record_span_error()
}

/// Escape control characters (C0, DEL, C1, which covers the ESC/CSI/OSC
/// introducers of terminal escape sequences) in user-supplied values before
/// they are written to the operator's terminal, so a crafted username or
/// email display name cannot erase, overwrite or recolour other rows.
fn sanitize_for_terminal(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() {
                c.escape_unicode().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

#[tracing::instrument(
    name = "delete-user",
    level = "info",
    skip(user_service, billing_service),
    fields({ attr::ERROR_TYPE } = tracing::field::Empty)
)]
pub async fn delete_user(
    user_service: Arc<UserService>,
    billing_service: Option<Arc<BillingService>>,
    user_id: UserId,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async move {
        let service = user_service.clone();

        let mut transaction = service.begin().await.context(format!(
            "Failed to create new transaction while deleting user {user_id}"
        ))?;

        service
            .delete_user(&mut transaction, user_id, billing_service.as_deref())
            .await?;

        transaction.commit().await.context(format!(
            "Failed to commit transaction while deleting user {user_id}"
        ))?;

        info!("User {user_id} and its data was successfully deleted");

        Ok(())
    }
    .await;
    result.record_span_error()
}

#[tracing::instrument(
    name = "reset-password-command",
    level = "info",
    skip(user_service, user_email),
    fields({ attr::ERROR_TYPE } = tracing::field::Empty)
)]
pub async fn reset_password(
    user_service: Arc<UserService>,
    user_email: &EmailAddress,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async move {
        let password_input = if io::stdin().is_terminal() {
            let p1 = rpassword::prompt_password(format!("New password for {user_email}: "))
                .context("Failed to read password")?;
            let p2 = rpassword::prompt_password("Confirm password: ")
                .context("Failed to read password confirmation")?;
            if p1 != p2 {
                return Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "Passwords do not match".to_string(),
                });
            }
            p1
        } else {
            let mut line = String::new();
            io::stdin()
                .lock()
                .read_line(&mut line)
                .context("Failed to read password from stdin")?;
            line.trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string()
        };

        let password: Password = password_input.parse().context("Invalid password")?;
        let password = SecretBox::new(Box::new(password));

        let service = user_service.clone();
        let mut transaction = service.begin().await.context(format!(
            "Failed to create new transaction while resetting password for {user_email}"
        ))?;

        let user = service
            .get_user_by_email(&mut transaction, user_email)
            .await?
            .context(format!(
                "Unable to find user with email address {user_email}"
            ))?;

        let has_local_auth = service
            .get_user_auth(&mut transaction, user.id, UserAuthKind::Local)
            .await?
            .is_some();

        if has_local_auth {
            service
                .set_password(&mut transaction, user.id, password)
                .await?;
            info!("Password updated for user {user_email}");
        } else {
            service
                .add_local_auth_method(&mut transaction, user.id, password)
                .await?;
            eprintln!(
                "Note: User {user_email} had no Local auth method. A new one has been created."
            );
            info!("Local auth method created with password for user {user_email}");
        }

        transaction.commit().await.context(format!(
            "Failed to commit transaction while resetting password for {user_email}"
        ))?;

        Ok(())
    }
    .await;
    result.record_span_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_for_terminal_escapes_control_sequences() {
        assert_eq!(sanitize_for_terminal("john@doe.name"), "john@doe.name");
        assert_eq!(
            sanitize_for_terminal("\ralex@corp.example"),
            "\\u{d}alex@corp.example"
        );
        assert_eq!(
            sanitize_for_terminal("\u{1b}[2Kroot\u{9b}31m"),
            "\\u{1b}[2Kroot\\u{9b}31m"
        );
        assert_eq!(sanitize_for_terminal("Zoë"), "Zoë");
    }
}
