use universal_inbox::{
    third_party::item::{ThirdPartyItem, ThirdPartyItemKind},
    user::UserId,
};

use universal_inbox_api::repository::third_party::ThirdPartyItemRepository;

use crate::helpers::TestedApp;

pub async fn find_third_party_items_for_user_id(
    app: &TestedApp,
    kind: ThirdPartyItemKind,
    user_id: UserId,
) -> Vec<ThirdPartyItem> {
    let mut transaction = app.repository.begin().await.unwrap();
    let third_party_items = app
        .repository
        .find_third_party_items_for_user_id(&mut transaction, kind, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    third_party_items
}
