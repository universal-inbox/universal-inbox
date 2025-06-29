use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{Notification, NotificationSource, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::api::WebPage,
        item::{ThirdPartyItem, ThirdPartyItemSourceKind},
    },
    user::UserId,
};
use uuid::Uuid;

use crate::{
    integrations::{
        notification::ThirdPartyNotificationSourceService, third_party::ThirdPartyItemSourceService,
    },
    universal_inbox::UniversalInboxError,
};

#[derive(Clone)]
pub struct APIService {}

impl APIService {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for APIService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ThirdPartyItemSourceService<WebPage> for APIService {
    async fn fetch_items(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        Ok(vec![])
    }

    fn is_sync_incremental(&self) -> bool {
        false
    }

    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::WebPage
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<WebPage> for APIService {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_third_party_item.source_id,
            third_party_item_id = source_third_party_item.id.to_string(),
            user.id = user_id.to_string(),
        ),
        err
    )]
    async fn third_party_item_into_notification(
        &self,
        source: &WebPage,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: source.title.clone(),
            status: NotificationStatus::Unread,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::API,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    async fn delete_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Do nothing as it does not exists as a source
        Ok(())
    }

    async fn unsubscribe_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // do nothing as it does not exists as a source
        Ok(())
    }

    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // do nothing as it does not exists as a source
        Ok(())
    }
}

impl IntegrationProviderSource for APIService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::API
    }
}

impl NotificationSource for APIService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::API
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
    }
}
