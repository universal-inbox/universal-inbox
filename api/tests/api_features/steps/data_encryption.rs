use std::sync::Arc;

use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use ring::{
    aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use secrecy::SecretBox;
use sqlx::Row;
use uuid::Uuid;

use email_address::EmailAddress;
use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::google_mail::GoogleMailConfig,
    },
    pii::Pii,
    third_party::{
        integrations::todoist::TodoistItem,
        item::{ThirdPartyItemData, ThirdPartyItemKind},
    },
    user::{PasswordHash, User},
};
use universal_inbox_api::{
    configuration::Settings,
    repository::{
        oauth_credential::OAuthCredentialRepository, task::TaskRepository,
        third_party::ThirdPartyItemRepository, user::UserRepository,
    },
    universal_inbox::{
        UniversalInboxError,
        data_encryption::{DataEncryptionService, DataEncryptionStatus, LEGACY_TOKEN_KEY_LABEL},
        user::model::{LocalUserAuth, UserAuth},
    },
    utils::crypto::{DataKeyring, decrypt_token},
};

use crate::{
    common::settings,
    helpers::{
        TestedApp,
        integration_connection::{
            create_integration_connection, google_account_id, todoist_oauth_credential,
        },
        task::todoist::{
            mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
        },
        third_party::create_task_third_party_item,
    },
    state::{RecordedProgress, StoredItem},
    world::ApiWorld,
};

const SECRET_MARKER: &str = "secret-content-marker";

const NEW_KEY: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

const ACCESS_TOKEN_COLUMN: &str = "oauth_credential.encrypted_access_token";

const GOOGLE_EMAIL_COLUMN: &str =
    "integration_connection.provider_user_id (Google email blind index)";

/// Data encryption key 1, as configured (the environment may override the test configuration)
fn key_1(settings: &Settings) -> &str {
    settings
        .data_encryption
        .keys
        .get("1")
        .map(String::as_str)
        .filter(|key| !key.trim().is_empty())
        .unwrap_or(&settings.oauth2.token_encryption_key)
}

fn current_keyring() -> Arc<DataKeyring> {
    Arc::new(settings().data_keyring().unwrap())
}

fn rotated_keyring() -> Arc<DataKeyring> {
    let settings = settings();
    Arc::new(DataKeyring::new(2, [(1, key_1(&settings)), (2, NEW_KEY)]).unwrap())
}

fn data_encryption_service(app: &TestedApp, keyring: Arc<DataKeyring>) -> DataEncryptionService {
    DataEncryptionService::new(app.repository.clone(), keyring)
}

/// OAuth token in the format used before the encryption envelope: nonce || ciphertext || tag
fn legacy_encrypt_token(plaintext: &str, aad: &[u8], hex_key: &str) -> Vec<u8> {
    let key =
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &hex::decode(hex_key).unwrap()).unwrap());
    let mut nonce = [0u8; 12];
    SystemRandom::new().fill(&mut nonce).unwrap();
    let mut in_out = plaintext.as_bytes().to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad),
        &mut in_out,
    )
    .unwrap();
    [nonce.to_vec(), in_out].concat()
}

async fn stored_access_token(app: &TestedApp, integration_connection_id: Uuid) -> Vec<u8> {
    sqlx::query(
        "SELECT encrypted_access_token FROM oauth_credential WHERE integration_connection_id = $1",
    )
    .bind(integration_connection_id)
    .fetch_one(&*app.repository.pool)
    .await
    .unwrap()
    .get("encrypted_access_token")
}

fn values_on_key(status: &DataEncryptionStatus, column: &str, key: &str) -> i64 {
    status
        .columns
        .iter()
        .find(|c| c.column == column)
        .and_then(|c| c.values_per_key.get(key).copied())
        .unwrap_or_default()
}

fn plaintext_values(status: &DataEncryptionStatus, column: &str) -> i64 {
    status
        .columns
        .iter()
        .find(|c| c.column == column)
        .map(|c| c.plaintext_values)
        .unwrap_or_default()
}

/// Values per column not encrypted with `active_key`, columns without such values omitted
fn values_not_on_key(
    status: &DataEncryptionStatus,
    active_key: &str,
) -> std::collections::BTreeMap<String, u64> {
    status
        .columns
        .iter()
        .map(|column| {
            let count: i64 = column
                .values_per_key
                .iter()
                .filter(|(key, _)| *key != active_key)
                .map(|(_, count)| count)
                .sum();
            (column.column.clone(), count as u64)
        })
        .filter(|(_, count)| *count > 0)
        .collect()
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn todoist_connection_id(world: &ApiWorld) -> Uuid {
    world
        .integrations
        .todoist_connection
        .as_ref()
        .expect("The account is not connected to Todoist")
        .id
        .0
}

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
    "the data encryption status counts 1 access token on the legacy key and no value it cannot read"
)]
async fn the_status_counts_the_legacy_access_token(world: &mut ApiWorld) {
    let keyring = current_keyring();
    let app = world.app.get().await;
    let status = data_encryption_service(app, keyring.clone())
        .status()
        .await
        .unwrap();
    assert_eq!(
        values_on_key(&status, ACCESS_TOKEN_COLUMN, LEGACY_TOKEN_KEY_LABEL),
        1
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
            .all(|column| encryption
                .progress
                .done
                .lock()
                .unwrap()
                .contains(&column.column))
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

#[given("Todoist can list my projects")]
async fn todoist_can_list_my_projects(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response(),
        None,
    )
    .await;
}

async fn store_todoist_item(world: &mut ApiWorld, item: TodoistItem) {
    let user_id = world.user().id;
    let data = ThirdPartyItemData::TodoistItem(Box::new(item));
    let app = world.app.get().await;
    let created = create_task_third_party_item(app, data.clone(), user_id).await;
    world.encryption.stored_item = Some(StoredItem {
        item: created.third_party_item,
        data,
        task: created.task,
    });
}

#[when("a Todoist task whose content contains the secret marker is stored")]
async fn a_todoist_task_with_a_secret_content_is_stored(world: &mut ApiWorld) {
    store_todoist_item(
        world,
        TodoistItem {
            project_id: "1111".to_string(),
            content: format!("Task {SECRET_MARKER}"),
            ..*todoist_item()
        },
    )
    .await;
}

#[when("a Todoist task whose description contains the secret marker is stored")]
async fn a_todoist_task_with_a_secret_description_is_stored(world: &mut ApiWorld) {
    let body = format!("Description {SECRET_MARKER}");
    world.encryption.expected_task_body = body.clone();
    store_todoist_item(
        world,
        TodoistItem {
            project_id: "1111".to_string(),
            description: body,
            ..*todoist_item()
        },
    )
    .await;
}

fn stored_item(world: &ApiWorld) -> &StoredItem {
    world
        .encryption
        .stored_item
        .as_ref()
        .expect("No item was stored")
}

#[then("the stored item data does not contain the secret marker")]
async fn the_stored_item_data_does_not_contain_the_marker(world: &mut ApiWorld) {
    let id = stored_item(world).item.id.0;
    let app = world.app.get().await;
    let data_enc: Vec<u8> =
        sqlx::query_scalar("SELECT data_enc FROM third_party_item WHERE id = $1")
            .bind(id)
            .fetch_one(&*app.repository.pool)
            .await
            .unwrap();
    assert!(!contains(&data_enc, SECRET_MARKER));
}

#[then("the data encryption status counts 1 third-party item value on the first key")]
async fn the_status_counts_one_item_on_the_first_key(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let status = data_encryption_service(app, current_keyring())
        .status()
        .await
        .unwrap();
    assert_eq!(values_on_key(&status, "third_party_item.data_enc", "1"), 1);
}

#[then("reading the item back gives the data that was stored")]
async fn reading_the_item_back_gives_the_data(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let (source_id, expected) = {
        let stored = stored_item(world);
        (stored.item.source_id.clone(), stored.data.clone())
    };
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let items = app
        .repository
        .find_third_party_items_for_source_id(
            &mut transaction,
            ThirdPartyItemKind::TodoistItem,
            &source_id,
            Some(user_id),
        )
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data, expected);
}

#[then("the task body is the description")]
fn the_task_body_is_the_description(world: &mut ApiWorld) {
    let task = stored_item(world)
        .task
        .as_ref()
        .expect("No task was created");
    assert_eq!(task.body, world.encryption.expected_task_body);
}

#[then("the stored task body is encrypted and absent from the search vector")]
async fn the_stored_task_body_is_encrypted(world: &mut ApiWorld) {
    let task_id = stored_item(world).task.as_ref().unwrap().id.0;
    let app = world.app.get().await;
    let row =
        sqlx::query("SELECT body_enc, title_project_tags_tsv::TEXT AS tsv FROM task WHERE id = $1")
            .bind(task_id)
            .fetch_one(&*app.repository.pool)
            .await
            .unwrap();
    assert!(!contains(&row.get::<Vec<u8>, _>("body_enc"), SECRET_MARKER));
    assert!(!row.get::<String, _>("tsv").contains("marker"));
}

#[then("reading the task back gives the same body")]
async fn reading_the_task_back_gives_the_same_body(world: &mut ApiWorld) {
    let task_id = stored_item(world).task.as_ref().unwrap().id;
    let expected = world.encryption.expected_task_body.clone();
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let stored_task = app
        .repository
        .get_one_task(&mut transaction, task_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_task.body, expected);
}

#[when("an OAuth credential whose raw token response holds the secret marker is stored")]
async fn an_oauth_credential_with_a_secret_response_is_stored(world: &mut ApiWorld) {
    let connection_id = world
        .integrations
        .todoist_connection
        .as_ref()
        .expect("The account is not connected to Todoist")
        .id;
    let raw_token_response = serde_json::json!({ "authed_user": { "id": SECRET_MARKER } });
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let stored = app
        .repository
        .store_oauth_credential(
            &mut transaction,
            connection_id,
            vec![1, 2, 3],
            None,
            None,
            raw_token_response.clone(),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.encryption.raw_token_response = Some((raw_token_response, stored.raw_token_response));
}

#[then("the stored credential reports the raw token response it was given")]
fn the_stored_credential_reports_the_response(world: &mut ApiWorld) {
    let (given, reported) = world.encryption.raw_token_response.as_ref().unwrap();
    assert_eq!(reported, given);
}

#[then("the raw token response is stored encrypted")]
async fn the_raw_token_response_is_stored_encrypted(world: &mut ApiWorld) {
    let connection_id = todoist_connection_id(world);
    let app = world.app.get().await;
    let encrypted: Vec<u8> = sqlx::query_scalar(
        "SELECT raw_token_response_enc FROM oauth_credential WHERE integration_connection_id = $1",
    )
    .bind(connection_id)
    .fetch_one(&*app.repository.pool)
    .await
    .unwrap();
    assert!(!contains(&encrypted, SECRET_MARKER));
}

#[then("reading the credential back gives the same raw token response")]
async fn reading_the_credential_back(world: &mut ApiWorld) {
    let connection_id = world.integrations.todoist_connection.as_ref().unwrap().id;
    let expected = world
        .encryption
        .raw_token_response
        .as_ref()
        .unwrap()
        .0
        .clone();
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let fetched = app
        .repository
        .get_oauth_credential(&mut transaction, connection_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.raw_token_response, expected);
}

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
    "the data encryption status counts 1 plaintext Google email value, as does the list of plaintext counts"
)]
async fn the_status_counts_one_plaintext_google_email(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let service = data_encryption_service(app, current_keyring());
    assert_eq!(
        plaintext_values(&service.status().await.unwrap(), GOOGLE_EMAIL_COLUMN),
        1
    );
    let plaintext_counts = service.plaintext_counts().await.unwrap();
    assert!(plaintext_counts.contains(&(GOOGLE_EMAIL_COLUMN.to_string(), 1)));
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
        *encryption.progress.done.lock().unwrap(),
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
