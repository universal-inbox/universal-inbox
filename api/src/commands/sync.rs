use crate::universal_inbox::{
    notification::{service::NotificationService, source::NotificationSource},
    UniversalInboxError,
};
use anyhow::Context;
use std::sync::Arc;

pub async fn sync(
    service: Arc<NotificationService>,
    source: &Option<NotificationSource>,
) -> Result<(), UniversalInboxError> {
    let transaction = service.repository.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &source
    ))?;

    service
        .sync_notifications(source)
        .await
        .context(format!("Failed to sync {:?}", &source))?;

    transaction
        .commit()
        .await
        .context(format!("Failed to commit while syncing {:?}", &source))?;
    Ok(())
}
