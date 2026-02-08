use std::sync::Arc;

use anyhow::Context;
use secrecy::{ExposeSecret, SecretBox};
use tracing::info;

use universal_inbox::user::Password;

use crate::universal_inbox::{UniversalInboxError, user::service::UserService};

const DEFAULT_PASSWORD: &str = "test123456";

#[tracing::instrument(name = "anonymize-database", level = "info", skip_all, err)]
pub async fn anonymize_database(user_service: Arc<UserService>) -> Result<(), UniversalInboxError> {
    let mut transaction = user_service
        .begin()
        .await
        .context("Failed to create new transaction while anonymizing database")?;

    info!("Generating password hash for default password");
    let password_hash = user_service.get_new_password_hash(SecretBox::new(Box::new(Password(
        DEFAULT_PASSWORD.to_string(),
    ))))?;
    let password_hash_str = password_hash.expose_secret().0.to_string();

    info!("Anonymizing user profiles");
    let result = sqlx::query(
        r#"
        UPDATE "user" SET
            email = 'test+' || id::text || '@test.com',
            first_name = 'Test',
            last_name = 'User',
            auth_user_id = 'test+' || id::text || '@test.com',
            email_validated_at = NOW(),
            updated_at = NOW()
        "#,
    )
    .execute(&mut *transaction)
    .await
    .context("Failed to anonymize user profiles")?;

    let user_count = result.rows_affected();

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
