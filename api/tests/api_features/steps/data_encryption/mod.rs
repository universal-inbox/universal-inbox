use std::sync::Arc;

use cucumber::{given, then, when};
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
    state::{RawTokenResponse, RecordedProgress, StoredItem},
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

mod email;
mod plaintext;
mod rotation;
mod stored_values;
