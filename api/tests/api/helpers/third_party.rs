use universal_inbox::{
    third_party::item::{ThirdPartyItemCreationResult, ThirdPartyItemData},
    user::UserId,
};

use super::TestedApp;

pub async fn create_task_third_party_item(
    app: &TestedApp,
    data: ThirdPartyItemData,
    user_id: UserId,
) -> Box<ThirdPartyItemCreationResult> {
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
