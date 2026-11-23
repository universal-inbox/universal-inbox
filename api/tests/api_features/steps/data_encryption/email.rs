use pretty_assertions::assert_eq;

use super::*;

#[then("my email address is stored encrypted, next to a blind index of its lowercase form")]
async fn my_email_is_stored_encrypted(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let lowercase = world.my_email().expose().to_string().to_lowercase();
    let app = world.app.get().await;
    let row = sqlx::query(r#"SELECT email_enc, email_hash FROM "user" WHERE id = $1"#)
        .bind(user_id.0)
        .fetch_one(&*app.repository.pool)
        .await
        .unwrap();
    assert!(!contains(&row.get::<Vec<u8>, _>("email_enc"), "Secret"));
    assert_eq!(
        row.get::<String, _>("email_hash"),
        google_account_id(&lowercase)
    );
}

#[then(expr = "a user with the email {string} cannot be created, as it already exists")]
async fn a_user_with_that_email_cannot_be_created(world: &mut ApiWorld, email: String) {
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let duplicate = app
        .repository
        .create_user(
            &mut transaction,
            User::new(None, None, Pii::new(email.parse::<EmailAddress>().unwrap())),
            UserAuth::Local(Box::new(LocalUserAuth {
                password_hash: SecretBox::new(Box::new(PasswordHash("hash".to_string()))),
                password_reset_at: None,
                password_reset_sent_at: None,
            })),
        )
        .await;
    assert!(matches!(
        duplicate,
        Err(UniversalInboxError::AlreadyExists { .. })
    ));
    transaction.rollback().await.unwrap();
}

#[then(expr = "looking up the address {string} finds me, with my email address as it was written")]
async fn looking_up_the_address_finds_me(world: &mut ApiWorld, address: String) {
    let user_id = world.user().id;
    let email = world.my_email();
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let stored_user = app
        .repository
        .get_user_by_email(
            &mut transaction,
            &Pii::new(address.parse::<EmailAddress>().unwrap()),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_user.id, user_id);
    assert_eq!(stored_user.email, Some(email));
}
