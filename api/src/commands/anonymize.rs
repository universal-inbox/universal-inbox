use std::sync::Arc;

use anyhow::Context;
use secrecy::{ExposeSecret, SecretBox};
use tracing::info;

use email_address::EmailAddress;
use uuid::Uuid;

use universal_inbox::{pii::Pii, user::Password};

use crate::observability::RecordSpanError;
use crate::observability::attr;
use crate::universal_inbox::{UniversalInboxError, user::service::UserService};

const DEFAULT_PASSWORD: &str = "test-password-123456";

#[tracing::instrument(name = "anonymize-database", level = "info", skip_all, fields({ attr::ERROR_TYPE } = tracing::field::Empty))]
pub async fn anonymize_database(user_service: Arc<UserService>) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async move {
        let mut transaction = user_service
            .begin()
            .await
            .context("Failed to create new transaction while anonymizing database")?;

        info!("Generating password hash for default password");
        let password_hash = user_service.get_new_password_hash(SecretBox::new(Box::new(
            Password(DEFAULT_PASSWORD.to_string()),
        )))?;
        let password_hash_str = password_hash.expose_secret().0.to_string();

        info!("Anonymizing user profiles");
        let user_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
        UPDATE "user" SET
            first_name = 'Test',
            last_name = 'User',
            updated_at = NOW()
        RETURNING id
        "#,
        )
        .fetch_all(&mut *transaction)
        .await
        .context("Failed to anonymize user profiles")?;
        // Emails are encrypted and indexed by the application, not in SQL
        for user_id in &user_ids {
            let email: Pii<EmailAddress> = Pii::new(
                format!("test+{user_id}@test.com")
                    .parse()
                    .context("Invalid anonymized email address")?,
            );
            user_service
                .set_verified_email(&mut transaction, (*user_id).into(), &email)
                .await?;
        }

        let user_count = user_ids.len();

        info!("Removing existing authentication records");
        sqlx::query("DELETE FROM user_auth")
            .execute(&mut *transaction)
            .await
            .context("Failed to delete existing user auth records")?;

        info!("Creating local authentication for all users");
        sqlx::query(
            r#"
        INSERT INTO user_auth (id, user_id, kind, password_hash)
        SELECT gen_random_uuid(), id, 'Local'::user_auth_kind, $1
        FROM "user"
        "#,
        )
        .bind(&password_hash_str)
        .execute(&mut *transaction)
        .await
        .context("Failed to create local auth records for all users")?;

        transaction
            .commit()
            .await
            .context("Failed to commit transaction while anonymizing database")?;

        info!("Database anonymized: {user_count} users updated with password {DEFAULT_PASSWORD}");

        Ok(())
    }
    .await;
    result.record_span_error()
}
