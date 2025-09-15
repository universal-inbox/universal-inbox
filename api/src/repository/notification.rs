use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{postgres::PgRow, FromRow, Postgres, QueryBuilder, Row, Transaction};
use tracing::debug;
use uuid::Uuid;

use universal_inbox::{
    notification::{
        service::NotificationPatch, Notification, NotificationId, NotificationListOrder,
        NotificationSourceKind, NotificationStatus, NotificationWithTask,
    },
    task::TaskId,
    third_party::item::ThirdPartyItemId,
    user::UserId,
    Page, PageToken, DEFAULT_PAGE_SIZE,
};

use crate::{
    repository::{task::TaskRow, Repository},
    universal_inbox::{UniversalInboxError, UpdateStatus, UpsertStatus},
};

use super::{third_party::ThirdPartyItemRow, FromRowWithPrefix};

#[async_trait]
pub trait NotificationRepository {
    async fn get_one_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: NotificationId,
    ) -> Result<Option<Notification>, UniversalInboxError>;
    async fn get_notification_for_source_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError>;
    async fn does_notification_exist(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: NotificationId,
    ) -> Result<bool, UniversalInboxError>;
    #[allow(clippy::too_many_arguments)]
    async fn fetch_all_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        status: Vec<NotificationStatus>,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
        order_by: NotificationListOrder,
        from_sources: Vec<NotificationSourceKind>,
        page_token: Option<PageToken>,
        user_id: UserId,
    ) -> Result<Page<NotificationWithTask>, UniversalInboxError>;
    async fn create_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError>;
    async fn update_stale_notifications_status_from_source_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        active_source_third_party_item_ids: Vec<ThirdPartyItemId>,
        kind: NotificationSourceKind,
        status: NotificationStatus,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn create_or_update_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification: Box<Notification>,
        kind: NotificationSourceKind,
        update_snoozed_until: bool,
    ) -> Result<UpsertStatus<Box<Notification>>, UniversalInboxError>;
    async fn update_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification_id: NotificationId,
        patch: &NotificationPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError>;
    async fn update_notifications_for_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        notification_kind: Option<NotificationSourceKind>,
        patch: &NotificationPatch,
    ) -> Result<Vec<UpdateStatus<Notification>>, UniversalInboxError>;
    async fn update_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        status: Vec<NotificationStatus>,
        from_sources: Vec<NotificationSourceKind>,
        patch: &NotificationPatch,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn delete_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        kind: NotificationSourceKind,
        user_id: UserId,
    ) -> Result<u64, UniversalInboxError>;
}

#[async_trait]
impl NotificationRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(notification_id = id.to_string()),
        err
    )]
    async fn get_one_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: NotificationId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let row = QueryBuilder::new(
            r#"
                SELECT
                  notification.id as notification__id,
                  notification.title as notification__title,
                  notification.status as notification__status,
                  notification.created_at as notification__created_at,
                  notification.updated_at as notification__updated_at,
                  notification.last_read_at as notification__last_read_at,
                  notification.snoozed_until as notification__snoozed_until,
                  notification.task_id as notification__task_id,
                  notification.user_id as notification__user_id,
                  notification.kind as notification__kind,
                  source_item.id as notification__source_item__id,
                  source_item.source_id as notification__source_item__source_id,
                  source_item.data as notification__source_item__data,
                  source_item.created_at as notification__source_item__created_at,
                  source_item.updated_at as notification__source_item__updated_at,
                  source_item.user_id as notification__source_item__user_id,
                  source_item.integration_connection_id as notification__source_item__integration_connection_id,
                  nested_source_item.id as notification__source_item__si__id,
                  nested_source_item.source_id as notification__source_item__si__source_id,
                  nested_source_item.data as notification__source_item__si__data,
                  nested_source_item.created_at as notification__source_item__si__created_at,
                  nested_source_item.updated_at as notification__source_item__si__updated_at,
                  nested_source_item.user_id as notification__source_item__si__user_id,
                  nested_source_item.integration_connection_id as notification__source_item__si__integration_connection_id
                FROM notification
                INNER JOIN third_party_item AS source_item
                  ON notification.source_item_id = source_item.id
                LEFT JOIN third_party_item AS nested_source_item
                  ON source_item.source_item_id = nested_source_item.id
                WHERE notification.id =
            "#,
        )
        .push_bind(id.0)
        .build_query_as::<NotificationRow>()
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch notification {id} from storage: {err}");
            UniversalInboxError::DatabaseError { source: err, message }
        })?;

        row.map(|notification_row| notification_row.try_into())
            .transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(source_id = source_id.to_string(), user.id = user_id.to_string()),
        err
    )]
    async fn get_notification_for_source_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let row = QueryBuilder::new(
            r#"
                SELECT
                  notification.id as notification__id,
                  notification.title as notification__title,
                  notification.status as notification__status,
                  notification.created_at as notification__created_at,
                  notification.updated_at as notification__updated_at,
                  notification.last_read_at as notification__last_read_at,
                  notification.snoozed_until as notification__snoozed_until,
                  notification.task_id as notification__task_id,
                  notification.user_id as notification__user_id,
                  notification.kind as notification__kind,
                  source_item.id as notification__source_item__id,
                  source_item.source_id as notification__source_item__source_id,
                  source_item.data as notification__source_item__data,
                  source_item.created_at as notification__source_item__created_at,
                  source_item.updated_at as notification__source_item__updated_at,
                  source_item.user_id as notification__source_item__user_id,
                  source_item.integration_connection_id as notification__source_item__integration_connection_id,
                  nested_source_item.id as notification__source_item__si__id,
                  nested_source_item.source_id as notification__source_item__si__source_id,
                  nested_source_item.data as notification__source_item__si__data,
                  nested_source_item.created_at as notification__source_item__si__created_at,
                  nested_source_item.updated_at as notification__source_item__si__updated_at,
                  nested_source_item.user_id as notification__source_item__si__user_id,
                  nested_source_item.integration_connection_id as notification__source_item__si__integration_connection_id
                FROM notification
                INNER JOIN third_party_item AS source_item
                  ON notification.source_item_id = source_item.id
                LEFT JOIN third_party_item AS nested_source_item
                  ON source_item.source_item_id = nested_source_item.id
                WHERE source_item.source_id =
            "#
        )
            .push_bind(source_id)
            .push(" AND notification.user_id = ")
            .push_bind(user_id.0)
            .build_query_as::<NotificationRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch notification for source ID {source_id} from storage: {err}");
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        row.map(|notification_row| notification_row.try_into())
            .transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(notification_id = id.to_string()),
        err
    )]
    async fn does_notification_exist(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: NotificationId,
    ) -> Result<bool, UniversalInboxError> {
        let count: Option<i64> =
            sqlx::query_scalar!(r#"SELECT count(*) FROM notification WHERE id = $1"#, id.0)
                .fetch_one(&mut **executor)
                .await
                .map_err(|err| {
                    let message = format!("Failed to check if notification {id} exists: {err}");
                    UniversalInboxError::DatabaseError {
                        source: err,
                        message,
                    }
                })?;

        if let Some(1) = count {
            return Ok(true);
        }
        return Ok(false);
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            status,
            include_snoozed_notifications,
            task_id = task_id.map(|id| id.to_string()),
            order_by,
            from_sources,
            page_token,
            user.id = user_id.to_string()
        ),
        err
    )]
    #[allow(clippy::too_many_arguments)]
    async fn fetch_all_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        status: Vec<NotificationStatus>,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
        order_by: NotificationListOrder,
        from_sources: Vec<NotificationSourceKind>,
        page_token: Option<PageToken>,
        user_id: UserId,
    ) -> Result<Page<NotificationWithTask>, UniversalInboxError> {
        fn add_filters(
            query_builder: &mut QueryBuilder<Postgres>,
            status: Vec<NotificationStatus>,
            include_snoozed_notifications: bool,
            task_id: Option<TaskId>,
            order_by: NotificationListOrder,
            from_sources: &[NotificationSourceKind],
            page_token: &Option<PageToken>,
            user_id: UserId,
        ) {
            let mut separated = query_builder.separated(" AND ");
            if !status.is_empty() {
                separated
                    .push("notification.status::TEXT = ANY(")
                    .push_bind_unseparated(
                        status
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<String>>(),
                    )
                    .push_unseparated(")");
            }
            separated
                .push(" notification.user_id = ")
                .push_bind_unseparated(user_id.0);

            if !include_snoozed_notifications {
                separated
                    .push(" (notification.snoozed_until is NULL OR notification.snoozed_until <=")
                    .push_bind_unseparated(Utc::now().naive_utc())
                    .push_unseparated(")");
            }

            if let Some(id) = task_id {
                separated
                    .push(" notification.task_id = ")
                    .push_bind_unseparated(id.0);
            }

            if !from_sources.is_empty() {
                let from_sources_str = from_sources
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>();
                separated
                    .push(" notification.kind::TEXT = ANY(")
                    .push_bind_unseparated(from_sources_str)
                    .push_unseparated(")");
            }

            match page_token {
                Some(PageToken::After(updated_at)) => {
                    separated
                        .push(format!(
                            " notification.{} ",
                            match order_by {
                                NotificationListOrder::UpdatedAtAsc => "updated_at >",
                                NotificationListOrder::UpdatedAtDesc => "updated_at <",
                            }
                        ))
                        .push_bind_unseparated(updated_at.naive_utc());
                }
                Some(PageToken::Before(updated_at)) => {
                    separated
                        .push(format!(
                            " notification.{} ",
                            match order_by {
                                NotificationListOrder::UpdatedAtAsc => "updated_at <",
                                NotificationListOrder::UpdatedAtDesc => "updated_at >",
                            }
                        ))
                        .push_bind_unseparated(updated_at.naive_utc());
                }
                _ => {}
            }
        }

        let mut count_query_builder =
            QueryBuilder::new(r#"SELECT count(*) FROM notification WHERE "#);

        add_filters(
            &mut count_query_builder,
            status.clone(),
            include_snoozed_notifications,
            task_id,
            order_by,
            &from_sources,
            &None,
            user_id,
        );

        let count = count_query_builder
            .build_query_scalar::<i64>()
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch notifications count from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  notification.id as notification__id,
                  notification.title as notification__title,
                  notification.status as notification__status,
                  notification.created_at as notification__created_at,
                  notification.updated_at as notification__updated_at,
                  notification.last_read_at as notification__last_read_at,
                  notification.snoozed_until as notification__snoozed_until,
                  notification.task_id as notification__task_id,
                  notification.user_id as notification__user_id,
                  notification.kind as notification__kind,
                  source_item.id as notification__source_item__id,
                  source_item.source_id as notification__source_item__source_id,
                  source_item.data as notification__source_item__data,
                  source_item.created_at as notification__source_item__created_at,
                  source_item.updated_at as notification__source_item__updated_at,
                  source_item.user_id as notification__source_item__user_id,
                  source_item.integration_connection_id as notification__source_item__integration_connection_id,
                  nested_source_item.id as notification__source_item__si__id,
                  nested_source_item.source_id as notification__source_item__si__source_id,
                  nested_source_item.data as notification__source_item__si__data,
                  nested_source_item.created_at as notification__source_item__si__created_at,
                  nested_source_item.updated_at as notification__source_item__si__updated_at,
                  nested_source_item.user_id as notification__source_item__si__user_id,
                  nested_source_item.integration_connection_id as notification__source_item__si__integration_connection_id,
                  task.id as notification__task__id,
                  task.title as notification__task__title,
                  task.body as notification__task__body,
                  task.status as notification__task__status,
                  task.completed_at as notification__task__completed_at,
                  task.priority as notification__task__priority,
                  task.due_at as notification__task__due_at,
                  task.tags as notification__task__tags,
                  task.parent_id as notification__task__parent_id,
                  task.project as notification__task__project,
                  task.is_recurring as notification__task__is_recurring,
                  task.created_at as notification__task__created_at,
                  task.updated_at as notification__task__updated_at,
                  task.kind::TEXT as notification__task__kind,
                  task.user_id as notification__task__user_id,
                  task_source_item.id as notification__task__source_item__id,
                  task_source_item.source_id as notification__task__source_item__source_id,
                  task_source_item.data as notification__task__source_item__data,
                  task_source_item.created_at as notification__task__source_item__created_at,
                  task_source_item.updated_at as notification__task__source_item__updated_at,
                  task_source_item.user_id as notification__task__source_item__user_id,
                  task_source_item.integration_connection_id as notification__task__source_item__integration_connection_id,
                  task_sink_item.id as notification__task__sink_item__id,
                  task_sink_item.source_id as notification__task__sink_item__source_id,
                  task_sink_item.data as notification__task__sink_item__data,
                  task_sink_item.created_at as notification__task__sink_item__created_at,
                  task_sink_item.updated_at as notification__task__sink_item__updated_at,
                  task_sink_item.user_id as notification__task__sink_item__user_id,
                  task_sink_item.integration_connection_id as notification__task__sink_item__integration_connection_id
                FROM
                  notification
                INNER JOIN third_party_item AS source_item
                  ON notification.source_item_id = source_item.id
                LEFT JOIN third_party_item AS nested_source_item
                  ON source_item.source_item_id = nested_source_item.id
                LEFT JOIN task ON task.id = notification.task_id
                LEFT JOIN third_party_item AS task_source_item
                  ON task.source_item_id = task_source_item.id
                LEFT JOIN third_party_item AS task_sink_item
                  ON task.sink_item_id = task_sink_item.id
                WHERE
            "#,
        );

        add_filters(
            &mut query_builder,
            status,
            include_snoozed_notifications,
            task_id,
            order_by,
            &from_sources,
            &page_token,
            user_id,
        );

        let reverse_order = matches!(page_token, Some(PageToken::Before(_)));
        let order_by_column = match order_by {
            NotificationListOrder::UpdatedAtAsc => format!(
                "notification.updated_at {}",
                if reverse_order { "DESC" } else { "ASC" }
            ),
            NotificationListOrder::UpdatedAtDesc => format!(
                "notification.updated_at {}",
                if reverse_order { "ASC" } else { "DESC" }
            ),
        };

        query_builder
            .push(format!(" ORDER BY {} ", order_by_column))
            .push(" LIMIT ")
            .push_bind(DEFAULT_PAGE_SIZE as i64);
        if let Some(PageToken::Offset(offset)) = page_token {
            query_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let records = query_builder
            .build_query_as::<NotificationWithTaskRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch notifications from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        let total: usize = count.try_into().unwrap(); // count(*) cannot be negative
        let mut content = records
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<NotificationWithTask>, UniversalInboxError>>()?;
        if reverse_order {
            content.reverse();
        }

        Ok(Page {
            per_page: DEFAULT_PAGE_SIZE,
            pages_count: total.div_ceil(DEFAULT_PAGE_SIZE),
            total,
            previous_page_token: content.first().map(|n| PageToken::Before(n.updated_at)),
            next_page_token: content.last().map(|n| PageToken::After(n.updated_at)),
            content,
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(notification_id = notification.id.to_string()),
        err
    )]
    async fn create_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        sqlx::query!(
            r#"
                INSERT INTO notification
                  (
                    id,
                    title,
                    status,
                    created_at,
                    updated_at,
                    last_read_at,
                    snoozed_until,
                    kind,
                    user_id,
                    task_id,
                    source_item_id
                  )
                VALUES
                  ($1, $2, $3::notification_status, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            notification.id.0,
            notification.title,
            notification.status.to_string() as _,
            notification.created_at.naive_utc(),
            notification.updated_at.naive_utc(),
            notification
                .last_read_at
                .map(|last_read_at| last_read_at.naive_utc()),
            notification
                .snoozed_until
                .map(|snoozed_until| snoozed_until.naive_utc()),
            notification.kind.to_string() as _,
            notification.user_id.0,
            notification.task_id.map(|task_id| task_id.0),
            notification.source_item.id.0,
        )
        .execute(&mut **executor)
        .await
        .map_err(|e| {
            match e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()))
            {
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: Some(e),
                    id: notification.id.0,
                },
                _ => UniversalInboxError::Unexpected(anyhow!(
                    "Failed to insert new notification into storage: {e}"
                )),
            }
        })?;

        Ok(notification)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            kind = kind.to_string(),
            status = status.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn update_stale_notifications_status_from_source_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        active_source_third_party_item_ids: Vec<ThirdPartyItemId>,
        kind: NotificationSourceKind,
        status: NotificationStatus,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE notification SET");
        query_builder
            .push(" status = ")
            .push_bind(status.to_string())
            .push("::notification_status");
        query_builder.push(
            r#"
                FROM
                  notification as n
                INNER JOIN third_party_item as source_item
                  ON n.source_item_id = source_item.id
                LEFT JOIN third_party_item AS nested_source_item
                  ON source_item.source_item_id = nested_source_item.id
                WHERE
                "#,
        );

        let mut separated = query_builder.separated(" AND ");
        separated.push(" notification.id = n.id ");
        let ids: Vec<String> = active_source_third_party_item_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        separated
            .push(" NOT source_item.id::TEXT = ANY(")
            .push_bind_unseparated(&ids)
            .push_unseparated(")");
        separated
            .push(" notification.kind::TEXT = ")
            .push_bind_unseparated(kind.to_string());
        separated
            .push(" (notification.status::TEXT = 'Read' OR notification.status::TEXT = 'Unread') ");
        separated
            .push(" notification.user_id = ")
            .push_bind_unseparated(user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  notification.id as notification__id,
                  notification.title as notification__title,
                  notification.status as notification__status,
                  notification.created_at as notification__created_at,
                  notification.updated_at as notification__updated_at,
                  notification.last_read_at as notification__last_read_at,
                  notification.snoozed_until as notification__snoozed_until,
                  notification.task_id as notification__task_id,
                  notification.user_id as notification__user_id,
                  notification.kind as notification__kind,
                  source_item.id as notification__source_item__id,
                  source_item.source_id as notification__source_item__source_id,
                  source_item.data as notification__source_item__data,
                  source_item.created_at as notification__source_item__created_at,
                  source_item.updated_at as notification__source_item__updated_at,
                  source_item.user_id as notification__source_item__user_id,
                  source_item.integration_connection_id as notification__source_item__integration_connection_id,
                  nested_source_item.id as notification__source_item__si__id,
                  nested_source_item.source_id as notification__source_item__si__source_id,
                  nested_source_item.data as notification__source_item__si__data,
                  nested_source_item.created_at as notification__source_item__si__created_at,
                  nested_source_item.updated_at as notification__source_item__si__updated_at,
                  nested_source_item.user_id as notification__source_item__si__user_id,
                  nested_source_item.integration_connection_id as notification__source_item__si__integration_connection_id
            "#,
        );

        let rows = query_builder
            .build_query_as::<NotificationRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to update stale notification status from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Notification>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification.id.to_string(),
            kind = kind.to_string(),
            update_snoozed_until
        ),
        err
    )]
    async fn create_or_update_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification: Box<Notification>,
        kind: NotificationSourceKind,
        update_snoozed_until: bool,
    ) -> Result<UpsertStatus<Box<Notification>>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  notification.id as notification__id,
                  notification.title as notification__title,
                  notification.status as notification__status,
                  notification.created_at as notification__created_at,
                  notification.updated_at as notification__updated_at,
                  notification.last_read_at as notification__last_read_at,
                  notification.snoozed_until as notification__snoozed_until,
                  notification.task_id as notification__task_id,
                  notification.user_id as notification__user_id,
                  notification.kind as notification__kind,
                  source_item.id as notification__source_item__id,
                  source_item.source_id as notification__source_item__source_id,
                  source_item.data as notification__source_item__data,
                  source_item.created_at as notification__source_item__created_at,
                  source_item.updated_at as notification__source_item__updated_at,
                  source_item.user_id as notification__source_item__user_id,
                  source_item.integration_connection_id as notification__source_item__integration_connection_id,
                  nested_source_item.id as notification__source_item__si__id,
                  nested_source_item.source_id as notification__source_item__si__source_id,
                  nested_source_item.data as notification__source_item__si__data,
                  nested_source_item.created_at as notification__source_item__si__created_at,
                  nested_source_item.updated_at as notification__source_item__si__updated_at,
                  nested_source_item.user_id as notification__source_item__si__user_id,
                  nested_source_item.integration_connection_id as notification__source_item__si__integration_connection_id
                FROM notification
                INNER JOIN third_party_item AS source_item
                  ON notification.source_item_id = source_item.id
                LEFT JOIN third_party_item AS nested_source_item
                  ON source_item.source_item_id = nested_source_item.id
                WHERE
            "#,
        );

        let mut separated = query_builder.separated(" AND ");
        separated
            .push(" notification.source_item_id = ")
            .push_bind_unseparated(notification.source_item.id.0)
            .push(" notification.kind::TEXT = ")
            .push_bind_unseparated(kind.to_string())
            .push(" notification.user_id = ")
            .push_bind_unseparated(notification.user_id.0);

        let existing_notification: Option<Notification> = query_builder
            .build_query_as::<NotificationRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to search for notification with source ID {} from storage: {err}",
                    notification.source_item.source_id
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?
            .map(TryInto::try_into)
            .transpose()?;

        let last_read_at_naive_utc = notification
            .last_read_at
            .map(|last_read_at| last_read_at.naive_utc());
        let snoozed_until_naive_utc = notification
            .snoozed_until
            .map(|snoozed_until| snoozed_until.naive_utc());

        if let Some(existing_notification) = existing_notification {
            if existing_notification == *notification {
                debug!(
                    "Existing {} notification {} (from {}) for {} does not need updating: {:?}",
                    kind,
                    existing_notification.id,
                    notification.source_item.source_id,
                    notification.user_id,
                    notification.updated_at
                );
                return Ok(UpsertStatus::Untouched(Box::new(existing_notification)));
            }

            debug!(
                "Updating existing {} notification {} (from {}) for {}",
                kind,
                existing_notification.id,
                notification.source_item.source_id,
                notification.user_id
            );
            let mut query_builder = QueryBuilder::new("UPDATE notification SET ");
            let mut separated = query_builder.separated(", ");
            separated
                .push("title = ")
                .push_bind_unseparated(notification.title.clone());
            separated
                .push("status = ")
                .push_bind_unseparated(notification.status.to_string())
                .push_unseparated("::notification_status");
            separated
                .push("updated_at = ")
                .push_bind_unseparated(notification.updated_at.naive_utc());
            separated
                .push("last_read_at = ")
                .push_bind_unseparated(last_read_at_naive_utc);
            if update_snoozed_until {
                separated
                    .push("snoozed_until = ")
                    .push_bind_unseparated(snoozed_until_naive_utc);
            }
            query_builder
                .push(" WHERE id = ")
                .push_bind(existing_notification.id.0);

            query_builder
                .build()
                .execute(&mut **executor)
                .await
                .map_err(|err| {
                    let message = format!(
                        "Failed to update notification {} from storage: {err}",
                        existing_notification.id
                    );
                    UniversalInboxError::DatabaseError {
                        source: err,
                        message,
                    }
                })?;

            return Ok(UpsertStatus::Updated {
                new: Box::new(Notification {
                    id: existing_notification.id,
                    kind: existing_notification.kind,
                    created_at: existing_notification.created_at,
                    source_item: existing_notification.source_item.clone(),
                    user_id: existing_notification.user_id,
                    task_id: existing_notification.task_id,
                    snoozed_until: if update_snoozed_until {
                        notification.snoozed_until
                    } else {
                        existing_notification.snoozed_until
                    },
                    ..*notification.clone()
                }),
                old: Box::new(existing_notification),
            });
        }

        debug!(
            "Creating new {} notification {} (from {}) for {}",
            kind, notification.id, notification.source_item.source_id, notification.user_id
        );
        let query = sqlx::query_scalar!(
            r#"
                INSERT INTO notification
                  (
                    id,
                    title,
                    status,
                    created_at,
                    updated_at,
                    last_read_at,
                    snoozed_until,
                    user_id,
                    task_id,
                    kind,
                    source_item_id
                  )
                VALUES
                  ($1, $2, $3::notification_status, $4, $5, $6, $7, $8, $9, $10::notification_source_kind, $11)
                RETURNING
                  id
                "#,
            notification.id.0, // no need to return the id as we already know it
            notification.title,
            notification.status.to_string() as _,
            notification.created_at.naive_utc(),
            notification.updated_at.naive_utc(),
            last_read_at_naive_utc,
            snoozed_until_naive_utc,
            notification.user_id.0,
            notification.task_id.map(|task_id| task_id.0),
            notification.kind.to_string() as _,
            notification.source_item.id.0,
        );

        let notification_id =
            NotificationId(query.fetch_one(&mut **executor).await.map_err(|err| {
                let message = format!(
                    "Failed to update notification with source ID {} from storage: {err}",
                    notification.source_item.source_id
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?);

        Ok(UpsertStatus::Created(Box::new(Notification {
            id: notification_id,
            ..*notification
        })))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification_id.to_string(),
            patch,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    async fn update_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification_id: NotificationId,
        patch: &NotificationPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        if *patch == Default::default() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!(
                    "Missing `status` field value to update notification {notification_id}"
                ),
            });
        };

        let mut query_builder = QueryBuilder::new("UPDATE notification SET");
        let mut separated = query_builder.separated(", ");
        if let Some(status) = patch.status {
            separated
                .push(" status = ")
                .push_bind_unseparated(status.to_string())
                .push_unseparated("::notification_status");
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" snoozed_until = ")
                .push_bind_unseparated(snoozed_until.naive_utc());
        }
        if let Some(task_id) = patch.task_id {
            separated
                .push(" task_id = ")
                .push_bind_unseparated(task_id.0);
        }

        query_builder
            .push(
                r#"
                FROM
                  notification as n
                INNER JOIN third_party_item AS source_item
                  ON n.source_item_id = source_item.id
                LEFT JOIN third_party_item AS nested_source_item
                  ON source_item.source_item_id = nested_source_item.id
                WHERE
              "#,
            )
            .separated(" AND ")
            .push(" notification.id = ")
            .push_bind_unseparated(notification_id.0)
            .push(" n.id = ")
            .push_bind_unseparated(notification_id.0)
            .push(" notification.user_id = ")
            .push_bind_unseparated(for_user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  notification.id as notification__id,
                  notification.title as notification__title,
                  notification.status as notification__status,
                  notification.created_at as notification__created_at,
                  notification.updated_at as notification__updated_at,
                  notification.last_read_at as notification__last_read_at,
                  notification.snoozed_until as notification__snoozed_until,
                  notification.task_id as notification__task_id,
                  notification.user_id as notification__user_id,
                  notification.kind as notification__kind,
                  source_item.id as notification__source_item__id,
                  source_item.source_id as notification__source_item__source_id,
                  source_item.data as notification__source_item__data,
                  source_item.created_at as notification__source_item__created_at,
                  source_item.updated_at as notification__source_item__updated_at,
                  source_item.user_id as notification__source_item__user_id,
                  source_item.integration_connection_id as notification__source_item__integration_connection_id,
                  nested_source_item.id as notification__source_item__si__id,
                  nested_source_item.source_id as notification__source_item__si__source_id,
                  nested_source_item.data as notification__source_item__si__data,
                  nested_source_item.created_at as notification__source_item__si__created_at,
                  nested_source_item.updated_at as notification__source_item__si__updated_at,
                  nested_source_item.user_id as notification__source_item__si__user_id,
                  nested_source_item.integration_connection_id as notification__source_item__si__integration_connection_id,
                  (SELECT"#,
        );

        let mut separated = query_builder.separated(" OR ");
        if let Some(status) = patch.status {
            separated
                .push(" status::TEXT != ")
                .push_bind_unseparated(status.to_string());
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" (snoozed_until is NULL OR snoozed_until != ")
                .push_bind_unseparated(snoozed_until.naive_utc())
                .push_unseparated(")");
        }
        if let Some(task_id) = patch.task_id {
            separated
                .push(" (task_id is NULL OR task_id != ")
                .push_bind_unseparated(task_id.0)
                .push_unseparated(")");
        }

        query_builder
            .push(" FROM notification WHERE id = ")
            .push_bind(notification_id.0);
        query_builder.push(r#") as "is_updated""#);

        let record: Option<UpdatedNotificationRow> = query_builder
            .build_query_as::<UpdatedNotificationRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to update notification {notification_id} from storage: {err}",);
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        if let Some(updated_notification_row) = record {
            Ok(UpdateStatus {
                updated: updated_notification_row.is_updated,
                result: Some(Box::new(
                    updated_notification_row
                        .notification_row
                        .try_into()
                        .unwrap(),
                )),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_id = task_id.to_string(),
            notification_kind = notification_kind.map(|kind| kind.to_string()),
            patch
        ),
        err
    )]
    async fn update_notifications_for_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        notification_kind: Option<NotificationSourceKind>,
        patch: &NotificationPatch,
    ) -> Result<Vec<UpdateStatus<Notification>>, UniversalInboxError> {
        if *patch == Default::default() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!(
                    "Missing `status` field value to update notifications for task {task_id}"
                ),
            });
        };

        let mut query_builder = QueryBuilder::new("UPDATE notification SET");
        let mut separated = query_builder.separated(", ");
        if let Some(status) = patch.status {
            separated
                .push(" status = ")
                .push_bind_unseparated(status.to_string())
                .push_unseparated("::notification_status");
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" snoozed_until = ")
                .push_bind_unseparated(snoozed_until.naive_utc());
        }

        query_builder.push(
            r#"
                FROM
                  notification as n
                INNER JOIN third_party_item AS source_item
                  ON n.source_item_id = source_item.id
                LEFT JOIN third_party_item AS nested_source_item
                  ON source_item.source_item_id = nested_source_item.id
                WHERE
              "#,
        );
        let mut separated = query_builder.separated(" AND ");
        separated
            .push(" notification.id = n.id")
            .push(" notification.task_id = ")
            .push_bind_unseparated(task_id.0);

        if let Some(kind) = notification_kind {
            separated
                .push(" notification.kind = ")
                .push_bind_unseparated(kind.to_string())
                .push_unseparated("::notification_source_kind");
        }

        query_builder.push(
            r#"
                RETURNING
                  notification.id as notification__id,
                  notification.title as notification__title,
                  notification.status as notification__status,
                  notification.created_at as notification__created_at,
                  notification.updated_at as notification__updated_at,
                  notification.last_read_at as notification__last_read_at,
                  notification.snoozed_until as notification__snoozed_until,
                  notification.task_id as notification__task_id,
                  notification.user_id as notification__user_id,
                  notification.kind as notification__kind,
                  source_item.id as notification__source_item__id,
                  source_item.source_id as notification__source_item__source_id,
                  source_item.data as notification__source_item__data,
                  source_item.created_at as notification__source_item__created_at,
                  source_item.updated_at as notification__source_item__updated_at,
                  source_item.user_id as notification__source_item__user_id,
                  source_item.integration_connection_id as notification__source_item__integration_connection_id,
                  nested_source_item.id as notification__source_item__si__id,
                  nested_source_item.source_id as notification__source_item__si__source_id,
                  nested_source_item.data as notification__source_item__si__data,
                  nested_source_item.created_at as notification__source_item__si__created_at,
                  nested_source_item.updated_at as notification__source_item__si__updated_at,
                  nested_source_item.user_id as notification__source_item__si__user_id,
                  nested_source_item.integration_connection_id as notification__source_item__si__integration_connection_id,
                  (SELECT"#,
        );

        let mut separated = query_builder.separated(" OR ");
        if let Some(status) = patch.status {
            separated
                .push(" status::TEXT != ")
                .push_bind_unseparated(status.to_string());
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" (snoozed_until is NULL OR snoozed_until != ")
                .push_bind_unseparated(snoozed_until.naive_utc())
                .push_unseparated(")");
        }

        query_builder
            .push(" FROM notification WHERE task_id = ")
            .push_bind(task_id.0);
        if let Some(kind) = notification_kind {
            query_builder
                .push(" AND kind::TEXT = ")
                .push_bind(kind.to_string());
        }
        query_builder.push(r#") as "is_updated""#);

        let records: Vec<UpdatedNotificationRow> = query_builder
            .build_query_as::<UpdatedNotificationRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to update notifications for task {task_id} from storage: {err}"
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        let update_statuses = records
            .into_iter()
            .map(|updated_notification_row| UpdateStatus {
                updated: updated_notification_row.is_updated,
                result: Some(
                    updated_notification_row
                        .notification_row
                        .try_into()
                        .unwrap(),
                ),
            })
            .collect();
        Ok(update_statuses)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            status = status.iter().map(|s| s.to_string()).collect::<Vec<String>>().join(","),
            from_sources,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn update_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        status: Vec<NotificationStatus>,
        from_sources: Vec<NotificationSourceKind>,
        patch: &NotificationPatch,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let Some(new_status) = patch.status else {
            return Ok(vec![]);
        };
        let mut query_builder = QueryBuilder::new("UPDATE notification SET");
        query_builder
            .push(" status = ")
            .push_bind(new_status.to_string())
            .push("::notification_status");
        query_builder.push(
            r#"
                FROM
                  notification as n
                INNER JOIN third_party_item as source_item
                  ON n.source_item_id = source_item.id
                LEFT JOIN third_party_item AS nested_source_item
                  ON source_item.source_item_id = nested_source_item.id
                WHERE
             "#,
        );

        let mut separated = query_builder.separated(" AND ");
        separated.push(" notification.id = n.id ");
        separated.push("notification.user_id = ");
        separated.push_bind_unseparated(user_id.0);

        let status_str: Vec<String> = status.into_iter().map(|s| s.to_string()).collect();
        if !status_str.is_empty() {
            separated.push(" notification.status::TEXT = ANY(");
            separated.push_bind_unseparated(&status_str);
            separated.push_unseparated(")");
        }

        let sources: Vec<String> = from_sources.into_iter().map(|s| s.to_string()).collect();
        if !sources.is_empty() {
            separated.push(" notification.kind::TEXT = ANY(");
            separated.push_bind_unseparated(&sources);
            separated.push_unseparated(")");
        }
        separated
            .push(" notification.user_id = ")
            .push_bind_unseparated(user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  notification.id as notification__id,
                  notification.title as notification__title,
                  notification.status as notification__status,
                  notification.created_at as notification__created_at,
                  notification.updated_at as notification__updated_at,
                  notification.last_read_at as notification__last_read_at,
                  notification.snoozed_until as notification__snoozed_until,
                  notification.task_id as notification__task_id,
                  notification.user_id as notification__user_id,
                  notification.kind as notification__kind,
                  source_item.id as notification__source_item__id,
                  source_item.source_id as notification__source_item__source_id,
                  source_item.data as notification__source_item__data,
                  source_item.created_at as notification__source_item__created_at,
                  source_item.updated_at as notification__source_item__updated_at,
                  source_item.user_id as notification__source_item__user_id,
                  source_item.integration_connection_id as notification__source_item__integration_connection_id,
                  nested_source_item.id as notification__source_item__si__id,
                  nested_source_item.source_id as notification__source_item__si__source_id,
                  nested_source_item.data as notification__source_item__si__data,
                  nested_source_item.created_at as notification__source_item__si__created_at,
                  nested_source_item.updated_at as notification__source_item__si__updated_at,
                  nested_source_item.user_id as notification__source_item__si__user_id,
                  nested_source_item.integration_connection_id as notification__source_item__si__integration_connection_id
            "#,
        );

        let rows = query_builder
            .build_query_as::<NotificationRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to update stale notification status from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Notification>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            kind = kind.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn delete_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        kind: NotificationSourceKind,
        user_id: UserId,
    ) -> Result<u64, UniversalInboxError> {
        let res = sqlx::query!(
            r#"
            DELETE FROM notification
            WHERE notification.kind::TEXT = $1
            AND notification.user_id = $2
            "#,
            kind.to_string(),
            user_id.0,
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to delete notifications for {kind} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(res.rows_affected())
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "notification_status")]
enum PgNotificationStatus {
    Unread,
    Read,
    Deleted,
    Unsubscribed,
}

impl TryFrom<&PgNotificationStatus> for NotificationStatus {
    type Error = UniversalInboxError;

    fn try_from(status: &PgNotificationStatus) -> Result<Self, Self::Error> {
        let status_str = format!("{status:?}");
        status_str
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: status_str,
            })
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "notification_source_kind")]
enum PgNotificationSourceKind {
    Github,
    Todoist,
    Linear,
    GoogleMail,
    GoogleCalendar,
    Slack,
    #[allow(clippy::upper_case_acronyms)]
    API,
}

impl TryFrom<&PgNotificationSourceKind> for NotificationSourceKind {
    type Error = UniversalInboxError;

    fn try_from(kind: &PgNotificationSourceKind) -> Result<Self, Self::Error> {
        let kind_str = format!("{kind:?}");
        kind_str
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: kind_str,
            })
    }
}

#[derive(Debug)]
struct NotificationRow {
    id: Uuid,
    title: String,
    status: PgNotificationStatus,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    last_read_at: Option<NaiveDateTime>,
    snoozed_until: Option<NaiveDateTime>,
    user_id: Uuid,
    task_id: Option<Uuid>,
    kind: PgNotificationSourceKind,
    source_item: ThirdPartyItemRow,
}

impl FromRow<'_, PgRow> for NotificationRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        NotificationRow::from_row_with_prefix(row, "notification__")
    }
}

impl FromRowWithPrefix<'_, PgRow> for NotificationRow {
    fn from_row_with_prefix(row: &PgRow, prefix: &str) -> sqlx::Result<Self> {
        Ok(NotificationRow {
            id: row.try_get(format!("{prefix}id").as_str())?,
            title: row.try_get(format!("{prefix}title").as_str())?,
            status: row
                .try_get::<PgNotificationStatus, &str>(format!("{prefix}status").as_str())?,
            created_at: row.try_get(format!("{prefix}created_at").as_str())?,
            updated_at: row.try_get(format!("{prefix}updated_at").as_str())?,
            last_read_at: row.try_get(format!("{prefix}last_read_at").as_str())?,
            snoozed_until: row.try_get(format!("{prefix}snoozed_until").as_str())?,
            user_id: row.try_get(format!("{prefix}user_id").as_str())?,
            task_id: row.try_get(format!("{prefix}task_id").as_str())?,
            kind: row
                .try_get::<PgNotificationSourceKind, &str>(format!("{prefix}kind").as_str())?,
            source_item: ThirdPartyItemRow::from_row_with_prefix(
                row,
                format!("{prefix}source_item__").as_str(),
            )?,
        })
    }
}

#[derive(Debug)]
struct NotificationWithTaskRow {
    id: Uuid,
    title: String,
    status: PgNotificationStatus,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    last_read_at: Option<NaiveDateTime>,
    snoozed_until: Option<NaiveDateTime>,
    user_id: Uuid,
    task: Option<TaskRow>,
    kind: PgNotificationSourceKind,
    source_item: ThirdPartyItemRow,
}

impl FromRow<'_, PgRow> for NotificationWithTaskRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        NotificationWithTaskRow::from_row_with_prefix(row, "notification__")
    }
}

impl FromRowWithPrefix<'_, PgRow> for NotificationWithTaskRow {
    fn from_row_with_prefix(row: &PgRow, prefix: &str) -> sqlx::Result<Self> {
        Ok(NotificationWithTaskRow {
            id: row.try_get(format!("{prefix}id").as_str())?,
            title: row.try_get(format!("{prefix}title").as_str())?,
            status: row
                .try_get::<PgNotificationStatus, &str>(format!("{prefix}status").as_str())?,
            created_at: row.try_get(format!("{prefix}created_at").as_str())?,
            updated_at: row.try_get(format!("{prefix}updated_at").as_str())?,
            last_read_at: row.try_get(format!("{prefix}last_read_at").as_str())?,
            snoozed_until: row.try_get(format!("{prefix}snoozed_until").as_str())?,
            user_id: row.try_get(format!("{prefix}user_id").as_str())?,
            task: row
                .try_get::<Option<Uuid>, &str>(format!("{prefix}task__id").as_str())?
                .map(|_task_id| {
                    TaskRow::from_row_with_prefix(row, format!("{prefix}task__").as_str())
                })
                .transpose()?,
            kind: row
                .try_get::<PgNotificationSourceKind, &str>(format!("{prefix}kind").as_str())?,
            source_item: ThirdPartyItemRow::from_row_with_prefix(
                row,
                format!("{prefix}source_item__").as_str(),
            )?,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct UpdatedNotificationRow {
    #[sqlx(flatten)]
    pub notification_row: NotificationRow,
    pub is_updated: bool,
}

impl TryFrom<NotificationRow> for Notification {
    type Error = UniversalInboxError;

    fn try_from(row: NotificationRow) -> Result<Self, Self::Error> {
        (&row).try_into()
    }
}

impl TryFrom<&NotificationRow> for Notification {
    type Error = UniversalInboxError;

    fn try_from(row: &NotificationRow) -> Result<Self, Self::Error> {
        let status = (&row.status).try_into()?;
        let kind = (&row.kind).try_into()?;

        Ok(Notification {
            id: row.id.into(),
            title: row.title.to_string(),
            status,
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
            last_read_at: row
                .last_read_at
                .map(|last_read_at| DateTime::from_naive_utc_and_offset(last_read_at, Utc)),
            snoozed_until: row
                .snoozed_until
                .map(|snoozed_until| DateTime::from_naive_utc_and_offset(snoozed_until, Utc)),
            user_id: row.user_id.into(),
            task_id: row.task_id.map(|task_id| task_id.into()),
            kind,
            source_item: row.source_item.clone().try_into()?,
        })
    }
}

impl TryFrom<&NotificationWithTaskRow> for NotificationWithTask {
    type Error = UniversalInboxError;

    fn try_from(row: &NotificationWithTaskRow) -> Result<Self, Self::Error> {
        let status = (&row.status).try_into()?;
        let kind = (&row.kind).try_into()?;

        Ok(NotificationWithTask {
            id: row.id.into(),
            title: row.title.to_string(),
            status,
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
            last_read_at: row
                .last_read_at
                .map(|last_read_at| DateTime::from_naive_utc_and_offset(last_read_at, Utc)),
            snoozed_until: row
                .snoozed_until
                .map(|snoozed_until| DateTime::from_naive_utc_and_offset(snoozed_until, Utc)),
            user_id: row.user_id.into(),
            task: row
                .task
                .as_ref()
                .map(|task_row| task_row.try_into())
                .transpose()?,
            kind,
            source_item: row.source_item.clone().try_into()?,
        })
    }
}
