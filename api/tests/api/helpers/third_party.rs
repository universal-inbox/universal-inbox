use universal_inbox::{
    third_party::item::{ThirdPartyItemCreationResult, ThirdPartyItemData},
    user::UserId,
};

use uuid::Uuid;

use universal_inbox_api::utils::crypto::{aad, data_keyring};

use super::{TestedApp, task::todoist::mirror_fixture_todoist_projects};

pub async fn create_task_third_party_item(
    app: &TestedApp,
    data: ThirdPartyItemData,
    user_id: UserId,
) -> Box<ThirdPartyItemCreationResult> {
    // As the Todoist sync does before the tasks: the projects mirrored as contexts, so the task is
    // bound to its project and the inbox is told by its context
    if matches!(data, ThirdPartyItemData::TodoistItem(_)) {
        mirror_fixture_todoist_projects(app, user_id).await;
    }
    let mut transaction = app.repository.begin().await.unwrap();
    let result = app
        .third_party_item_service
        .read()
        .await
        .create_task_item(&mut transaction, data, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    Box::new(result.expect("create_task_item returned None"))
}

/// Overwrite the stored (encrypted) data of a third party item with `json`, eg. to simulate
/// data that no longer matches the current types.
pub async fn overwrite_third_party_item_data(app: &TestedApp, id: Uuid, json: &str) {
    let data_enc = data_keyring()
        .unwrap()
        .encrypt_compressed(json.as_bytes(), &aad("third_party_item.data", id))
        .unwrap();
    sqlx::query("UPDATE third_party_item SET data_enc = $1 WHERE id = $2")
        .bind(data_enc)
        .bind(id)
        .execute(&*app.repository.pool)
        .await
        .expect("Failed to overwrite the third party item data");
}
