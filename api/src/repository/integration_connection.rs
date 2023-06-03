use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        IntegrationProviderKind,
    },
    user::UserId,
};

use crate::{
    repository::Repository,
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

#[async_trait]
pub trait IntegrationConnectionRepository {
    async fn get_integration_connection<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError>;

    async fn get_integration_connection_per_provider<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        for_user_id: UserId,
        integration_provider_kind: IntegrationProviderKind,
        synced_before: Option<DateTime<Utc>>,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError>;

    async fn update_integration_connection_status<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        new_status: IntegrationConnectionStatus,
        failure_message: Option<String>,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError>;

    async fn update_integration_connection_sync_status<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
        integration_provider_kind: IntegrationProviderKind,
        failure_message: Option<String>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError>;

    async fn fetch_all_integration_connections<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        for_user_id: UserId,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError>;

    async fn create_integration_connection<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_connection: Box<IntegrationConnection>,
    ) -> Result<Box<IntegrationConnection>, UniversalInboxError>;
}

#[async_trait]
impl IntegrationConnectionRepository for Repository {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_integration_connection<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        let row = sqlx::query_as!(
            IntegrationConnectionRow,
            r#"
                SELECT
                  id,
                  user_id,
                  connection_id,
                  provider_kind as "provider_kind: _",
                  status as "status: _",
                  failure_message,
                  created_at,
                  updated_at,
                  last_sync_started_at,
                  last_sync_failure_message
                FROM integration_connection
                WHERE id = $1
            "#,
            integration_connection_id.0
        )
        .fetch_optional(executor)
        .await
        .with_context(|| {
            format!(
                "Failed to fetch integration connection {integration_connection_id} from storage"
            )
        })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_integration_connection_per_provider<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
        integration_provider_kind: IntegrationProviderKind,
        synced_before: Option<DateTime<Utc>>,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  id,
                  user_id,
                  connection_id,
                  provider_kind,
                  status,
                  failure_message,
                  created_at,
                  updated_at,
                  last_sync_started_at,
                  last_sync_failure_message
                FROM integration_connection
                WHERE
            "#,
        );
        let mut separated = query_builder.separated(" AND ");
        separated
            .push("user_id = ")
            .push_bind_unseparated(user_id.0);
        separated
            .push("provider_kind::TEXT = ")
            .push_bind_unseparated(integration_provider_kind.to_string());

        if let Some(synced_before) = synced_before {
            separated
                .push("(last_sync_started_at is null OR last_sync_started_at <= ")
                .push_bind_unseparated(synced_before)
                .push_unseparated(")");
        }

        let row: Option<IntegrationConnectionRow> = query_builder
            .build_query_as::<IntegrationConnectionRow>()
            .fetch_optional(executor)
            .await
            .with_context(|| {
                format!("Failed to fetch integration connection for user {user_id} of kind {integration_provider_kind} from storage")
            })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn update_integration_connection_status<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        new_status: IntegrationConnectionStatus,
        failure_message: Option<String>,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE integration_connection SET");
        let mut separated = query_builder.separated(", ");
        separated
            .push(" status = ")
            .push_bind_unseparated(new_status.to_string())
            .push_unseparated("::integration_connection_status");
        separated
            .push(" failure_message = ")
            .push_bind_unseparated(failure_message.clone());

        query_builder
            .push(" WHERE ")
            .separated(" AND ")
            .push(" id = ")
            .push_bind_unseparated(integration_connection_id.0)
            .push(" user_id = ")
            .push_bind_unseparated(for_user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  id,
                  user_id,
                  connection_id,
                  provider_kind,
                  status,
                  failure_message,
                  created_at,
                  updated_at,
                  last_sync_started_at,
                  last_sync_failure_message,
                  (SELECT
             "#,
        );

        let mut separated = query_builder.separated(" OR ");
        separated
            .push(" status::TEXT != ")
            .push_bind_unseparated(new_status.to_string());
        if let Some(failure_message) = failure_message {
            separated
                .push(" (failure_message IS NULL OR failure_message != ")
                .push_bind_unseparated(failure_message)
                .push_unseparated(")");
        } else {
            separated.push(" failure_message IS NOT NULL");
        }

        query_builder
            .push(" FROM integration_connection WHERE id = ")
            .push_bind(integration_connection_id.0)
            .push(r#") as "is_updated""#);

        let row: Option<UpdatedIntegrationConnectionRow> = query_builder
            .build_query_as::<UpdatedIntegrationConnectionRow>()
            .fetch_optional(executor)
            .await
            .with_context(|| {
                format!("Failed to update integration connection {integration_connection_id} from storage")
            })?;

        if let Some(updated_integration_connection_row) = row {
            Ok(UpdateStatus {
                updated: updated_integration_connection_row.is_updated,
                result: Some(Box::new(
                    updated_integration_connection_row
                        .integration_connection_row
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

    #[tracing::instrument(level = "debug", skip(self))]
    async fn update_integration_connection_sync_status<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
        integration_provider_kind: IntegrationProviderKind,
        failure_message: Option<String>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE integration_connection SET");
        let mut separated = query_builder.separated(", ");
        separated
            .push(" last_sync_started_at = ")
            .push_bind_unseparated(Utc::now());
        separated
            .push(" last_sync_failure_message = ")
            .push_bind_unseparated(failure_message.clone());

        query_builder
            .push(" WHERE ")
            .separated(" AND ")
            .push(" user_id = ")
            .push_bind_unseparated(user_id.0)
            .push(" provider_kind::TEXT = ")
            .push_bind_unseparated(integration_provider_kind.to_string());

        query_builder.push(
            r#"
                RETURNING
                  id,
                  user_id,
                  connection_id,
                  provider_kind,
                  status,
                  failure_message,
                  created_at,
                  updated_at,
                  last_sync_started_at,
                  last_sync_failure_message,
                  true as "is_updated"
             "#,
        );

        let row: Option<UpdatedIntegrationConnectionRow> = query_builder
            .build_query_as::<UpdatedIntegrationConnectionRow>()
            .fetch_optional(executor)
            .await
            .with_context(|| {
                format!("Failed to update integration connection {integration_provider_kind} for user {user_id} from storage")
            })?;

        if let Some(updated_integration_connection_row) = row {
            Ok(UpdateStatus {
                updated: updated_integration_connection_row.is_updated,
                result: Some(Box::new(
                    updated_integration_connection_row
                        .integration_connection_row
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

    #[tracing::instrument(level = "debug", skip(self))]
    async fn fetch_all_integration_connections<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        for_user_id: UserId,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError> {
        let rows = sqlx::query_as!(
            IntegrationConnectionRow,
            r#"
                SELECT
                  id,
                  user_id,
                  connection_id,
                  provider_kind as "provider_kind: _",
                  status as "status: _",
                  failure_message,
                  created_at,
                  updated_at,
                  last_sync_started_at,
                  last_sync_failure_message
                FROM integration_connection
                WHERE user_id = $1
            "#,
            for_user_id.0
        )
        .fetch_all(executor)
        .await
        .context("Failed to fetch all integration connections from storage")?;

        rows.into_iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<IntegrationConnection>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create_integration_connection<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_connection: Box<IntegrationConnection>,
    ) -> Result<Box<IntegrationConnection>, UniversalInboxError> {
        sqlx::query!(
            r#"
                INSERT INTO integration_connection
                  (
                    id,
                    user_id,
                    connection_id,
                    provider_kind,
                    status,
                    failure_message,
                    created_at,
                    updated_at
                  )
                VALUES
                  (
                    $1,
                    $2,
                    $3,
                    $4::integration_provider_kind,
                    $5::integration_connection_status,
                    $6,
                    $7,
                    $8
                  )
            "#,
            integration_connection.id.0,
            integration_connection.user_id.0,
            integration_connection.connection_id.0,
            integration_connection.provider_kind.to_string() as _,
            integration_connection.status.to_string() as _,
            integration_connection.failure_message,
            integration_connection.created_at.naive_utc(),
            integration_connection.updated_at.naive_utc()
        )
        .execute(executor)
        .await
        .map_err(|e| {
            match e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()))
            {
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: e,
                    id: integration_connection.id.0,
                },
                _ => UniversalInboxError::Unexpected(anyhow!(
                    "Failed to insert new integration connection into storage"
                )),
            }
        })?;

        Ok(integration_connection)
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "integration_connection_status")]
enum PgIntegrationConnectionStatus {
    Created,
    Validated,
    Failing,
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "integration_provider_kind")]
enum PgIntegrationProviderKind {
    Github,
    Todoist,
}

#[derive(Debug, sqlx::FromRow)]
pub struct IntegrationConnectionRow {
    id: Uuid,
    user_id: Uuid,
    connection_id: Uuid,
    provider_kind: PgIntegrationProviderKind,
    status: PgIntegrationConnectionStatus,
    failure_message: Option<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    last_sync_started_at: Option<NaiveDateTime>,
    last_sync_failure_message: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct UpdatedIntegrationConnectionRow {
    #[sqlx(flatten)]
    pub integration_connection_row: IntegrationConnectionRow,
    pub is_updated: bool,
}

impl TryFrom<&PgIntegrationConnectionStatus> for IntegrationConnectionStatus {
    type Error = UniversalInboxError;

    fn try_from(status: &PgIntegrationConnectionStatus) -> Result<Self, Self::Error> {
        let status_str = format!("{status:?}");
        status_str
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: status_str,
            })
    }
}

impl TryFrom<&PgIntegrationProviderKind> for IntegrationProviderKind {
    type Error = UniversalInboxError;

    fn try_from(provider_kind: &PgIntegrationProviderKind) -> Result<Self, Self::Error> {
        let provider_kind_str = format!("{provider_kind:?}");
        provider_kind_str
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: provider_kind_str,
            })
    }
}

impl TryFrom<IntegrationConnectionRow> for IntegrationConnection {
    type Error = UniversalInboxError;
    fn try_from(row: IntegrationConnectionRow) -> Result<Self, Self::Error> {
        let status = (&row.status).try_into()?;
        let provider_kind = (&row.provider_kind).try_into()?;

        Ok(IntegrationConnection {
            id: row.id.into(),
            user_id: row.user_id.into(),
            connection_id: row.connection_id.into(),
            provider_kind,
            status,
            failure_message: row.failure_message,
            created_at: DateTime::<Utc>::from_utc(row.created_at, Utc),
            updated_at: DateTime::<Utc>::from_utc(row.updated_at, Utc),
            last_sync_started_at: row
                .last_sync_started_at
                .map(|started_at| DateTime::<Utc>::from_utc(started_at, Utc)),
            last_sync_failure_message: row.last_sync_failure_message,
        })
    }
}
