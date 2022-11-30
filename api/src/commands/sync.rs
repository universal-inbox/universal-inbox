use std::sync::Arc;

use anyhow::Context;

use crate::universal_inbox::{
    notification::{service::NotificationService, source::NotificationSourceKind},
    UniversalInboxError,
};

pub async fn sync(
    service: Arc<NotificationService>,
    source: &Option<NotificationSourceKind>,
) -> Result<(), UniversalInboxError> {
    let transactional_service = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &source
    ))?;

    transactional_service
        .sync_notifications(source)
        .await
        .context(format!("Failed to sync {:?}", &source))?;

    transactional_service
        .commit()
        .await
        .context(format!("Failed to commit while syncing {:?}", &source))?;

    Ok(())
}
