//! Tests of the data encryption maintenance commands (`data-encryption status`,
//! `encrypt-plaintext`, `reencrypt`).

use std::sync::Arc;

use email_address::EmailAddress;
use reqwest::Client;
use ring::{
    aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use rstest::rstest;
use secrecy::SecretBox;
use sqlx::Row;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{google_mail::GoogleMailConfig, todoist::TodoistConfig},
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
    integrations::todoist::TodoistSyncResponse,
    repository::{
        oauth_credential::OAuthCredentialRepository, task::TaskRepository,
        third_party::ThirdPartyItemRepository, user::UserRepository,
    },
    universal_inbox::{
        UniversalInboxError,
        data_encryption::{DataEncryptionService, LEGACY_TOKEN_KEY_LABEL},
        user::model::{LocalUserAuth, UserAuth},
    },
    utils::crypto::{DataKeyring, decrypt_token},
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection,
        create_integration_connection, google_account_id, todoist_oauth_credential,
    },
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
    tested_app_with_local_auth,
    third_party::create_task_third_party_item,
    user::{create_user_and_login, login_user_response},
};

const SECRET_MARKER: &str = "secret-content-marker";

const NEW_KEY: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

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

fn rotated_keyring(settings: &Settings) -> Arc<DataKeyring> {
    Arc::new(DataKeyring::new(2, [(1, key_1(settings)), (2, NEW_KEY)]).unwrap())
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

fn values_on_key(
    status: &universal_inbox_api::universal_inbox::data_encryption::DataEncryptionStatus,
    column: &str,
    key: &str,
) -> i64 {
    status
        .columns
        .iter()
        .find(|c| c.column == column)
        .and_then(|c| c.values_per_key.get(key).copied())
        .unwrap_or_default()
}

#[rstest]
#[tokio::test]
async fn test_reencrypt_oauth_tokens_with_rotated_key(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let access_token = todoist_oauth_credential.access_token.as_str().to_string();
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    let connection_id = integration_connection.id.0;
    // Token stored before the encryption envelope existed
    sqlx::query("UPDATE oauth_credential SET encrypted_access_token = $1 WHERE integration_connection_id = $2")
        .bind(legacy_encrypt_token(
            &access_token,
            connection_id.as_bytes(),
            key_1(&settings),
        ))
        .bind(connection_id)
        .execute(&*app.app.repository.pool)
        .await
        .unwrap();

    let current_keyring = Arc::new(settings.data_keyring().unwrap());
    let status = data_encryption_service(&app.app, current_keyring.clone())
        .status()
        .await
        .unwrap();
    assert_eq!(
        values_on_key(
            &status,
            "oauth_credential.encrypted_access_token",
            LEGACY_TOKEN_KEY_LABEL
        ),
        1
    );
    assert!(status.unreadable_values(&current_keyring).is_empty());

    let rotated_keyring = rotated_keyring(&settings);
    let status = data_encryption_service(&app.app, rotated_keyring.clone())
        .reencrypt(1)
        .await
        .unwrap();

    assert_eq!(
        values_on_key(&status, "oauth_credential.encrypted_access_token", "2"),
        1
    );
    assert_eq!(
        status.values_per_key().keys().collect::<Vec<_>>(),
        vec!["2"]
    );
    let stored = stored_access_token(&app.app, connection_id).await;
    assert_eq!(
        decrypt_token(&stored, connection_id.as_bytes(), &rotated_keyring).unwrap(),
        access_token
    );

    // Once rotated, the old key alone cannot read the data: starting with it is refused
    let error = data_encryption_service(&app.app, current_keyring)
        .encrypt_plaintext(10)
        .await
        .unwrap_err();
    assert!(format!("{error:?}").contains("oauth_credential.encrypted_access_token"));

    // Re-encrypting again is a no-op
    let status_again = data_encryption_service(&app.app, rotated_keyring)
        .reencrypt(1)
        .await
        .unwrap();
    assert_eq!(status_again, status);
}

fn plaintext_values(
    status: &universal_inbox_api::universal_inbox::data_encryption::DataEncryptionStatus,
    column: &str,
) -> i64 {
    status
        .columns
        .iter()
        .find(|c| c.column == column)
        .map(|c| c.plaintext_values)
        .unwrap_or_default()
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[rstest]
#[tokio::test]
async fn test_encrypt_plaintext_third_party_item_data(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;
    let data = ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
        project_id: "1111".to_string(),
        content: format!("Task {SECRET_MARKER}"),
        ..*todoist_item.clone()
    }));
    let item = create_task_third_party_item(&app.app, data.clone(), app.user.id)
        .await
        .third_party_item;

    // Stored encrypted only
    let row =
        sqlx::query("SELECT data::TEXT AS data, data_enc FROM third_party_item WHERE id = $1")
            .bind(item.id.0)
            .fetch_one(&*app.app.repository.pool)
            .await
            .unwrap();
    assert_eq!(row.get::<Option<String>, _>("data"), None);
    let data_enc: Vec<u8> = row.get("data_enc");
    assert!(!contains(&data_enc, SECRET_MARKER));

    // Row stored before encryption existed
    sqlx::query("UPDATE third_party_item SET data = $1::TEXT::JSON, data_enc = NULL WHERE id = $2")
        .bind(serde_json::to_string(&data).unwrap())
        .bind(item.id.0)
        .execute(&*app.app.repository.pool)
        .await
        .unwrap();
    let keyring = Arc::new(settings.data_keyring().unwrap());
    let service = data_encryption_service(&app.app, keyring);
    let status = service.status().await.unwrap();
    assert_eq!(plaintext_values(&status, "third_party_item.data_enc"), 1);

    let status = service.encrypt_plaintext(1).await.unwrap();

    assert_eq!(status.plaintext_values(), 0);
    assert_eq!(values_on_key(&status, "third_party_item.data_enc", "1"), 1);
    let row =
        sqlx::query("SELECT data::TEXT AS data, data_enc FROM third_party_item WHERE id = $1")
            .bind(item.id.0)
            .fetch_one(&*app.app.repository.pool)
            .await
            .unwrap();
    assert_eq!(row.get::<Option<String>, _>("data"), None);
    assert!(!contains(&row.get::<Vec<u8>, _>("data_enc"), SECRET_MARKER));
    let mut transaction = app.app.repository.begin().await.unwrap();
    let items = app
        .app
        .repository
        .find_third_party_items_for_source_id(
            &mut transaction,
            ThirdPartyItemKind::TodoistItem,
            &item.source_id,
            Some(app.user.id),
        )
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data, data);

    // Idempotent
    assert_eq!(service.encrypt_plaintext(1).await.unwrap(), status);
}

#[rstest]
#[tokio::test]
async fn test_encrypt_plaintext_task_body(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;
    let body = format!("Description {SECRET_MARKER}");
    let task = create_task_third_party_item(
        &app.app,
        ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
            project_id: "1111".to_string(),
            description: body.clone(),
            ..*todoist_item.clone()
        })),
        app.user.id,
    )
    .await
    .task
    .unwrap();
    assert_eq!(task.body, body);

    // Stored encrypted only, and not in the search vector
    let row = sqlx::query(
        "SELECT body, body_enc, title_project_tags_tsv::TEXT AS tsv FROM task WHERE id = $1",
    )
    .bind(task.id.0)
    .fetch_one(&*app.app.repository.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<Option<String>, _>("body"), None);
    assert!(!contains(&row.get::<Vec<u8>, _>("body_enc"), SECRET_MARKER));
    assert!(!row.get::<String, _>("tsv").contains("marker"));

    // Row stored before encryption existed
    sqlx::query("UPDATE task SET body = $1, body_enc = NULL WHERE id = $2")
        .bind(&body)
        .bind(task.id.0)
        .execute(&*app.app.repository.pool)
        .await
        .unwrap();
    let service = data_encryption_service(&app.app, Arc::new(settings.data_keyring().unwrap()));
    assert_eq!(
        plaintext_values(&service.status().await.unwrap(), "task.body_enc"),
        1
    );

    let status = service.encrypt_plaintext(10).await.unwrap();

    assert_eq!(status.plaintext_values(), 0);
    let row = sqlx::query("SELECT body, body_enc FROM task WHERE id = $1")
        .bind(task.id.0)
        .fetch_one(&*app.app.repository.pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Option<String>, _>("body"), None);
    assert!(!contains(&row.get::<Vec<u8>, _>("body_enc"), SECRET_MARKER));
    let mut transaction = app.app.repository.begin().await.unwrap();
    let stored_task = app
        .app
        .repository
        .get_one_task(&mut transaction, task.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_task.body, body);
}

#[rstest]
#[tokio::test]
async fn test_raw_token_response_is_encrypted(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    let raw_token_response = serde_json::json!({ "authed_user": { "id": SECRET_MARKER } });

    let mut transaction = app.app.repository.begin().await.unwrap();
    let stored = app
        .app
        .repository
        .store_oauth_credential(
            &mut transaction,
            integration_connection.id,
            vec![1, 2, 3],
            None,
            None,
            raw_token_response.clone(),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert_eq!(stored.raw_token_response, raw_token_response);
    let row = sqlx::query(
        "SELECT raw_token_response, raw_token_response_enc FROM oauth_credential WHERE integration_connection_id = $1",
    )
    .bind(integration_connection.id.0)
    .fetch_one(&*app.app.repository.pool)
    .await
    .unwrap();
    assert_eq!(
        row.get::<Option<serde_json::Value>, _>("raw_token_response"),
        None
    );
    assert!(!contains(
        &row.get::<Vec<u8>, _>("raw_token_response_enc"),
        SECRET_MARKER
    ));
    let mut transaction = app.app.repository.begin().await.unwrap();
    let fetched = app
        .app
        .repository
        .get_oauth_credential(&mut transaction, integration_connection.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.raw_token_response, raw_token_response);
}

#[rstest]
#[tokio::test]
async fn test_user_email_is_encrypted_and_looked_up_case_insensitively(
    settings: Settings,
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let email: Pii<EmailAddress> = Pii::new("Jane.Secret@Example.com".parse().unwrap());
    let (_client, user) = create_user_and_login(&app, email.clone(), "password-12345678").await;

    // Stored encrypted, with its blind index
    let row = sqlx::query(r#"SELECT email, email_enc, email_hash FROM "user" WHERE id = $1"#)
        .bind(user.id.0)
        .fetch_one(&*app.repository.pool)
        .await
        .unwrap();
    assert_eq!(row.get::<Option<String>, _>("email"), None);
    assert!(!contains(&row.get::<Vec<u8>, _>("email_enc"), "Secret"));
    assert_eq!(
        row.get::<String, _>("email_hash"),
        google_account_id("jane.secret@example.com")
    );

    // Login matches the address whatever its case
    let client = Client::builder().cookie_store(true).build().unwrap();
    let response = login_user_response(
        &client,
        &app,
        Pii::new("jane.secret@EXAMPLE.com".parse().unwrap()),
        "password-12345678",
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let mut transaction = app.repository.begin().await.unwrap();
    let duplicate = app
        .repository
        .create_user(
            &mut transaction,
            User::new(
                None,
                None,
                Pii::new("JANE.SECRET@example.com".parse().unwrap()),
            ),
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

    // Row stored before encryption existed
    sqlx::query(
        r#"UPDATE "user" SET email = 'Jane.Secret@Example.com', email_enc = NULL, email_hash = NULL WHERE id = $1"#,
    )
    .bind(user.id.0)
    .execute(&*app.repository.pool)
    .await
    .unwrap();
    let service = data_encryption_service(&app, Arc::new(settings.data_keyring().unwrap()));
    assert_eq!(
        plaintext_values(&service.status().await.unwrap(), "user.email_enc"),
        1
    );

    let status = service.encrypt_plaintext(10).await.unwrap();

    assert_eq!(status.plaintext_values(), 0);
    let mut transaction = app.repository.begin().await.unwrap();
    let stored_user = app
        .repository
        .get_user_by_email(
            &mut transaction,
            &Pii::new("jane.secret@example.com".parse().unwrap()),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_user.id, user.id);
    assert_eq!(stored_user.email, Some(email));
}

#[rstest]
#[tokio::test]
async fn test_encrypt_plaintext_google_account_email(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    // Google connection pinned before email addresses were encrypted
    let integration_connection = create_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        IntegrationConnectionStatus::Validated,
        None,
        Some("john.roe@example.com".to_string()),
        None,
        None,
        None,
    )
    .await;
    let service = data_encryption_service(&app.app, Arc::new(settings.data_keyring().unwrap()));
    assert_eq!(
        plaintext_values(
            &service.status().await.unwrap(),
            "integration_connection.provider_user_id (Google email blind index)"
        ),
        1
    );

    let status = service.encrypt_plaintext(10).await.unwrap();

    assert_eq!(status.plaintext_values(), 0);
    let provider_user_id: Option<String> =
        sqlx::query_scalar("SELECT provider_user_id FROM integration_connection WHERE id = $1")
            .bind(integration_connection.id.0)
            .fetch_one(&*app.app.repository.pool)
            .await
            .unwrap();
    assert_eq!(
        provider_user_id,
        Some(google_account_id("john.roe@example.com"))
    );
}
