use universal_inbox_api::universal_inbox::UniversalInboxError;

use super::encryption::StoredItem;

#[derive(Default)]
pub struct ThirdPartyItems {
    pub created: Option<StoredItem>,
    pub refusal: Option<UniversalInboxError>,
}
