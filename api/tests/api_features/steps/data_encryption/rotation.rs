use pretty_assertions::assert_eq;

use super::*;

#[given("my Todoist access token was stored before the encryption envelope existed")]
async fn my_access_token_was_stored_in_the_legacy_format(world: &mut ApiWorld) {
    let connection_id = todoist_connection_id(world);
    let settings = settings();
    let app = world.app.get().await;
    sqlx::query(
        "UPDATE oauth_credential SET encrypted_access_token = $1 WHERE integration_connection_id = $2",
    )
    .bind(legacy_encrypt_token(
        todoist_oauth_credential().access_token.as_str(),
        connection_id.as_bytes(),
        key_1(&settings),
    ))
    .bind(connection_id)
    .execute(&*app.repository.pool)
    .await
    .unwrap();
}

#[then(
    expr = "the data encryption status counts {int} access token(s) on the legacy key and no value it cannot read"
)]
async fn the_status_counts_the_legacy_access_token(world: &mut ApiWorld, count: i64) {
    let keyring = current_keyring();
    let app = world.app.get().await;
    let status = data_encryption_service(app, keyring.clone())
        .status()
        .await
        .unwrap();
    assert_eq!(
        values_on_key(&status, ACCESS_TOKEN_COLUMN, LEGACY_TOKEN_KEY_LABEL),
        count
    );
    assert!(status.unreadable_values(&keyring).is_empty());
    world.encryption.status_before = Some(status);
}

#[when("the keys are rotated to a second key and all data is re-encrypted")]
async fn the_keys_are_rotated(world: &mut ApiWorld) {
    let progress = RecordedProgress::default();
    let app = world.app.get().await;
    let status = data_encryption_service(app, rotated_keyring())
        .reencrypt_with_progress(1, &progress)
        .await
        .unwrap();
    world.encryption.progress = progress;
    world.encryption.status = Some(status);
}

#[then(
    "every value that was not on the new key was reported as re-encrypted, and every column as done"
)]
fn every_value_was_reported(world: &mut ApiWorld) {
    let encryption = &world.encryption;
    let status = encryption.status.as_ref().unwrap();
    assert_eq!(
        encryption.progress.processed(),
        values_not_on_key(encryption.status_before.as_ref().unwrap(), "2")
    );
    assert!(encryption.progress.processed()[ACCESS_TOKEN_COLUMN] >= 1);
    assert!(
        status
            .columns
            .iter()
            .filter(|column| !column.column.contains("Google email blind index"))
            .all(|column| encryption.progress.done().contains(&column.column))
    );
}

#[then("all data is on the new key, including my access token")]
fn all_data_is_on_the_new_key(world: &mut ApiWorld) {
    let status = world.encryption.status.as_ref().unwrap();
    assert_eq!(values_on_key(status, ACCESS_TOKEN_COLUMN, "2"), 1);
    assert_eq!(
        status.values_per_key().keys().collect::<Vec<_>>(),
        vec!["2"]
    );
}

#[then("my access token can be read with the new key")]
async fn my_access_token_can_be_read_with_the_new_key(world: &mut ApiWorld) {
    let connection_id = todoist_connection_id(world);
    let app = world.app.get().await;
    let stored = stored_access_token(app, connection_id).await;
    assert_eq!(
        decrypt_token(&stored, connection_id.as_bytes(), &rotated_keyring()).unwrap(),
        todoist_oauth_credential().access_token.as_str()
    );
}

#[then("starting with the old key alone is refused, naming the column of the access tokens")]
async fn starting_with_the_old_key_is_refused(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let error = data_encryption_service(app, current_keyring())
        .encrypt_plaintext(10)
        .await
        .unwrap_err();
    assert!(format!("{error:?}").contains(ACCESS_TOKEN_COLUMN));
}

#[then("re-encrypting again changes nothing")]
async fn re_encrypting_again_changes_nothing(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let status_again = data_encryption_service(app, rotated_keyring())
        .reencrypt(1)
        .await
        .unwrap();
    assert_eq!(&status_again, world.encryption.status.as_ref().unwrap());
}
