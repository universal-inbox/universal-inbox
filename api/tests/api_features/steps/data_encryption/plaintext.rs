use pretty_assertions::assert_eq;

use super::*;

#[given(
    expr = "I have a Google Mail connection pinned to the address {string} before addresses were encrypted"
)]
async fn i_have_a_google_mail_connection_pinned_to_an_address(
    world: &mut ApiWorld,
    address: String,
) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        IntegrationConnectionStatus::Validated,
        None,
        Some(address),
        None,
        None,
        None,
    )
    .await;
    world.integrations.google_connection = Some(connection);
}

#[then(
    expr = "the data encryption status counts {int} plaintext Google email value(s), as does the list of plaintext counts"
)]
async fn the_status_counts_plaintext_google_emails(world: &mut ApiWorld, count: i64) {
    let app = world.app.get().await;
    let service = data_encryption_service(app, current_keyring());
    assert_eq!(
        plaintext_values(&service.status().await.unwrap(), GOOGLE_EMAIL_COLUMN),
        count
    );
    let plaintext_counts = service.plaintext_counts().await.unwrap();
    assert!(plaintext_counts.contains(&(GOOGLE_EMAIL_COLUMN.to_string(), count)));
    world.encryption.plaintext_counts = plaintext_counts;
}

#[when("all plaintext values are encrypted")]
async fn all_plaintext_values_are_encrypted(world: &mut ApiWorld) {
    let progress = RecordedProgress::default();
    let app = world.app.get().await;
    let status = data_encryption_service(app, current_keyring())
        .encrypt_plaintext_with_progress(10, &progress)
        .await
        .unwrap();
    world.encryption.progress = progress;
    world.encryption.status = Some(status);
}

#[then("the progress totals are the plaintext counts and every counted column is done")]
fn the_progress_totals_are_the_plaintext_counts(world: &mut ApiWorld) {
    let encryption = &world.encryption;
    assert_eq!(
        encryption.progress.processed(),
        encryption
            .plaintext_counts
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(column, count)| (column.clone(), *count as u64))
            .collect()
    );
    assert_eq!(
        encryption.progress.done(),
        encryption
            .plaintext_counts
            .iter()
            .map(|(column, _)| column.clone())
            .collect()
    );
}

#[then("no plaintext value is left")]
fn no_plaintext_value_is_left(world: &mut ApiWorld) {
    assert_eq!(
        world.encryption.status.as_ref().unwrap().plaintext_values(),
        0
    );
}

#[then(expr = "the Google Mail connection is pinned to the blind index of {string}")]
async fn the_connection_is_pinned_to_the_blind_index(world: &mut ApiWorld, address: String) {
    let id = world.integrations.google_connection.as_ref().unwrap().id.0;
    let app = world.app.get().await;
    let provider_user_id: Option<String> =
        sqlx::query_scalar("SELECT provider_user_id FROM integration_connection WHERE id = $1")
            .bind(id)
            .fetch_one(&*app.repository.pool)
            .await
            .unwrap();
    assert_eq!(provider_user_id, Some(google_account_id(&address)));
}
