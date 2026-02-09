use anyhow::{Context, Result, anyhow};
use dioxus::prelude::*;

use futures_util::StreamExt;
use log::{debug, error};
use reqwest::Method;
use url::Url;

use universal_inbox::{
    IntegrationProviderStaticConfig,
    integration_connection::{
        IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionId,
        IntegrationConnectionStatus, NangoPublicKey, config::IntegrationConnectionConfig,
        provider::IntegrationProviderKind,
    },
};

use crate::{
    components::toast_zone::{Toast, ToastKind},
    config::AppConfig,
    model::{LoadState, UniversalInboxUIModel},
    services::{
        api::call_api, nango::nango_auth, notification_service::NotificationCommand,
        task_service::TaskCommand, toast_service::ToastCommand,
    },
};

#[derive(Debug)]
pub enum IntegrationConnectionCommand {
    Refresh,
    CreateIntegrationConnection(IntegrationProviderKind),
    UpdateIntegrationConnectionConfig(IntegrationConnection, IntegrationConnectionConfig),
    AuthenticateIntegrationConnection(IntegrationConnection),
    DisconnectIntegrationConnection(IntegrationConnectionId),
    ReconnectIntegrationConnection(IntegrationConnection),
}

pub static INTEGRATION_CONNECTIONS: GlobalSignal<Option<Vec<IntegrationConnection>>> =
    Signal::global(|| None);
pub static TASK_SERVICE_INTEGRATION_CONNECTION: GlobalSignal<
    LoadState<Option<IntegrationConnection>>,
> = Signal::global(|| LoadState::None);

#[allow(clippy::too_many_arguments)]
pub async fn integration_connnection_service(
    mut rx: UnboundedReceiver<IntegrationConnectionCommand>,
    app_config: ReadSignal<Option<AppConfig>>,
    integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
    notification_service: Coroutine<NotificationCommand>,
    task_service: Coroutine<TaskCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(IntegrationConnectionCommand::Refresh) => {
                if let Err(error) = refresh_integration_connection(
                    integration_connections,
                    task_service_integration_connection,
                    app_config,
                    ui_model,
                    &notification_service,
                    &task_service,
                )
                .await
                {
                    error!("An error occurred while refreshing integration connections: {error:?}");
                }
            }
            Some(IntegrationConnectionCommand::CreateIntegrationConnection(
                integration_provider_kind,
            )) => {
                match create_integration_connection(
                    integration_provider_kind,
                    integration_connections,
                    task_service_integration_connection,
                    app_config,
                    ui_model,
                    &notification_service,
                    &task_service,
                )
                .await
                {
                    Ok(integration_connection) => sync_integration_connection(
                        &integration_connection,
                        &notification_service,
                        &task_service,
                    ),
                    Err(error) => {
                        error!(
                            "An error occurred while connecting with {integration_provider_kind}: {error:?}"
                        );
                        toast_service.send(ToastCommand::Push(Toast {
                            kind: ToastKind::Failure,
                            message: format!("An error occurred while connecting with {integration_provider_kind}. Please, retry 🙏 If the issue keeps happening, please contact our support."),
                            timeout: Some(10_000),
                            ..Default::default()
                        }));
                    }
                }
            }
            Some(IntegrationConnectionCommand::UpdateIntegrationConnectionConfig(
                integration_connection,
                config,
            )) => {
                match update_integration_connection_config(
                    integration_connection.id,
                    config,
                    integration_connections,
                    task_service_integration_connection,
                    app_config,
                    ui_model,
                    &notification_service,
                    &task_service,
                )
                .await
                {
                    Ok(()) => sync_integration_connection(
                        &integration_connection,
                        &notification_service,
                        &task_service,
                    ),
                    Err(error) => {
                        error!(
                            "An error occurred while updating integration connection {} configuration: {error:?}",
                            integration_connection.id
                        );
                        toast_service.send(ToastCommand::Push(Toast {
                            kind: ToastKind::Failure,
                            message: format!(
                                "An error occurred while updating integration connection {}. Please, retry 🙏 If the issue keeps happening, please contact our support.",
                                integration_connection.id
                            ),
                            timeout: Some(10_000),
                            ..Default::default()
                        }));
                    }
                }
            }
            Some(IntegrationConnectionCommand::AuthenticateIntegrationConnection(
                integration_connection,
            )) => {
                match authenticate_integration_connection(
                    &integration_connection,
                    integration_connections,
                    task_service_integration_connection,
                    app_config,
                    ui_model,
                    &notification_service,
                    &task_service,
                )
                .await
                {
                    Ok(integration_connection) => sync_integration_connection(
                        &integration_connection,
                        &notification_service,
                        &task_service,
                    ),
                    Err(error) => {
                        let provider_kind = integration_connection.provider.kind();
                        error!(
                            "An error occurred while authenticating with {provider_kind}: {error:?}"
                        );
                        toast_service.send(ToastCommand::Push(Toast {
                            kind: ToastKind::Failure,
                            message: format!(
                                "An error occurred while authenticating with {provider_kind}. Please, retry 🙏 If the issue keeps happening, please contact our support."
                            ),
                            timeout: Some(10_000),
                            ..Default::default()
                        }));
                    }
                }
            }
            Some(IntegrationConnectionCommand::DisconnectIntegrationConnection(
                integration_connection_id,
            )) => {
                let _result = disconnect_integration_connection(
                    integration_connection_id,
                    integration_connections,
                    task_service_integration_connection,
                    app_config,
                    ui_model,
                    &notification_service,
                    &task_service,
                )
                .await;
            }
            Some(IntegrationConnectionCommand::ReconnectIntegrationConnection(
                integration_connection,
            )) => {
                match reconnect_integration_connection(
                    &integration_connection,
                    integration_connections,
                    task_service_integration_connection,
                    app_config,
                    ui_model,
                    &notification_service,
                    &task_service,
                )
                .await
                {
                    Ok(integration_connection) => sync_integration_connection(
                        &integration_connection,
                        &notification_service,
                        &task_service,
                    ),
                    Err(error) => {
                        let provider_kind = integration_connection.provider.kind();
                        error!(
                            "An error occurred while reconnecting with {provider_kind}: {error:?}"
                        );
                        toast_service.send(ToastCommand::Push(Toast {
                            kind: ToastKind::Failure,
                            message: format!(
                                "An error occurred while reconnecting with {provider_kind}. Please, retry 🙏 If the issue keeps happening, please contact our support."
                            ),
                            timeout: Some(10_000),
                            ..Default::default()
                        }));
                    }
                }
            }
            None => {}
        }
    }
}

async fn refresh_integration_connection(
    mut integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
    mut task_service_integration_connection_ref: Signal<LoadState<Option<IntegrationConnection>>>,
    app_config: ReadSignal<Option<AppConfig>>,
    mut ui_model: Signal<UniversalInboxUIModel>,
    notification_service: &Coroutine<NotificationCommand>,
    task_service: &Coroutine<TaskCommand>,
) -> Result<()> {
    let api_base_url = get_api_base_url(app_config)?;

    let new_integration_connections: Vec<IntegrationConnection> = call_api(
        Method::GET,
        &api_base_url,
        "integration-connections",
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model),
    )
    .await?;

    let task_service_integration_connection = new_integration_connections
        .iter()
        .find(|c| c.is_connected_task_service());
    ui_model.write().is_task_actions_enabled = task_service_integration_connection.is_some();
    *task_service_integration_connection_ref.write() =
        LoadState::Loaded(task_service_integration_connection.cloned());

    let was_syncing = ui_model.read().is_syncing_notifications || ui_model.read().is_syncing_tasks;
    ui_model.write().is_syncing_notifications = new_integration_connections
        .iter()
        .any(|c| c.is_syncing_notifications());
    ui_model.write().is_syncing_tasks = new_integration_connections
        .iter()
        .any(|c| c.is_syncing_tasks());

    if was_syncing
        && !(ui_model.read().is_syncing_notifications || ui_model.read().is_syncing_tasks)
    {
        debug!("Sync completed, refreshing notifications");
        notification_service.send(NotificationCommand::Refresh);
        task_service.send(TaskCommand::RefreshSyncedTasks);
    }

    *integration_connections.write() = Some(new_integration_connections);

    Ok(())
}

async fn create_integration_connection(
    integration_provider_kind: IntegrationProviderKind,
    mut integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    app_config: ReadSignal<Option<AppConfig>>,
    ui_model: Signal<UniversalInboxUIModel>,
    notification_service: &Coroutine<NotificationCommand>,
    task_service: &Coroutine<TaskCommand>,
) -> Result<IntegrationConnection> {
    let api_base_url = get_api_base_url(app_config)?;

    debug!("Creating new integration connection for {integration_provider_kind}");
    let new_connection: IntegrationConnection = call_api(
        Method::POST,
        &api_base_url,
        "integration-connections",
        Some(IntegrationConnectionCreation {
            provider_kind: integration_provider_kind,
        }),
        Some(ui_model),
    )
    .await?;

    {
        let mut integration_connections = integration_connections.write();
        if let Some(integration_connections) = integration_connections.as_mut() {
            integration_connections.push(new_connection.clone());
        } else {
            *integration_connections = Some(vec![new_connection.clone()]);
        }
    }

    authenticate_integration_connection(
        &new_connection,
        integration_connections,
        task_service_integration_connection,
        app_config,
        ui_model,
        notification_service,
        task_service,
    )
    .await
}

async fn authenticate_integration_connection(
    integration_connection: &IntegrationConnection,
    integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    app_config: ReadSignal<Option<AppConfig>>,
    ui_model: Signal<UniversalInboxUIModel>,
    notification_service: &Coroutine<NotificationCommand>,
    task_service: &Coroutine<TaskCommand>,
) -> Result<IntegrationConnection> {
    let provider_kind = integration_connection.provider.kind();
    let (nango_base_url, nango_public_key, provider_config) =
        get_configs(app_config, provider_kind)?;

    debug!(
        "Authenticating integration_connection {} for {provider_kind}",
        integration_connection.id
    );
    nango_auth(
        &nango_base_url,
        &nango_public_key,
        &provider_config.nango_config_key,
        &integration_connection.connection_id,
        provider_config.oauth_user_scopes.clone(),
    )
    .await?;

    verify_integration_connection(
        integration_connection.id,
        integration_connections,
        task_service_integration_connection,
        app_config,
        ui_model,
        notification_service,
        task_service,
    )
    .await
}

async fn verify_integration_connection(
    integration_connection_id: IntegrationConnectionId,
    integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    app_config: ReadSignal<Option<AppConfig>>,
    ui_model: Signal<UniversalInboxUIModel>,
    notification_service: &Coroutine<NotificationCommand>,
    task_service: &Coroutine<TaskCommand>,
) -> Result<IntegrationConnection> {
    let api_base_url = get_api_base_url(app_config)?;

    debug!("Verifying integration connection {integration_connection_id}");
    let result: IntegrationConnection = call_api(
        Method::PATCH,
        &api_base_url,
        &format!("integration-connections/{integration_connection_id}/status"),
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model),
    )
    .await?;

    update_integration_connection_status(
        result.id,
        result.status,
        result.failure_message.clone(),
        integration_connections,
    );

    refresh_integration_connection(
        integration_connections,
        task_service_integration_connection,
        app_config,
        ui_model,
        notification_service,
        task_service,
    )
    .await?;

    Ok(result)
}

async fn disconnect_integration_connection(
    integration_connection_id: IntegrationConnectionId,
    integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    app_config: ReadSignal<Option<AppConfig>>,
    ui_model: Signal<UniversalInboxUIModel>,
    notification_service: &Coroutine<NotificationCommand>,
    task_service: &Coroutine<TaskCommand>,
) -> Result<()> {
    let api_base_url = get_api_base_url(app_config)?;

    debug!("Disconnecting integration connection {integration_connection_id}");
    let result: IntegrationConnection = call_api(
        Method::DELETE,
        &api_base_url,
        &format!("integration-connections/{integration_connection_id}"),
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model),
    )
    .await?;

    update_integration_connection_status(
        result.id,
        result.status,
        result.failure_message,
        integration_connections,
    );

    refresh_integration_connection(
        integration_connections,
        task_service_integration_connection,
        app_config,
        ui_model,
        notification_service,
        task_service,
    )
    .await
}

async fn reconnect_integration_connection(
    integration_connection: &IntegrationConnection,
    integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    app_config: ReadSignal<Option<AppConfig>>,
    ui_model: Signal<UniversalInboxUIModel>,
    notification_service: &Coroutine<NotificationCommand>,
    task_service: &Coroutine<TaskCommand>,
) -> Result<IntegrationConnection> {
    disconnect_integration_connection(
        integration_connection.id,
        integration_connections,
        task_service_integration_connection,
        app_config,
        ui_model,
        notification_service,
        task_service,
    )
    .await?;

    authenticate_integration_connection(
        integration_connection,
        integration_connections,
        task_service_integration_connection,
        app_config,
        ui_model,
        notification_service,
        task_service,
    )
    .await
}

fn update_integration_connection_status(
    integration_connection_id: IntegrationConnectionId,
    status: IntegrationConnectionStatus,
    failure_message: Option<String>,
    mut integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
) {
    debug!("Updating integration connection {integration_connection_id} status with {status}");
    if let Some(integration_connections) = integration_connections.write().as_mut()
        && let Some(integration_connection) = integration_connections
            .iter_mut()
            .find(|integration_connection| integration_connection.id == integration_connection_id)
    {
        integration_connection.status = status;
        integration_connection.failure_message = failure_message;
    }
}

fn sync_integration_connection(
    integration_connection: &IntegrationConnection,
    notification_service: &Coroutine<NotificationCommand>,
    task_service: &Coroutine<TaskCommand>,
) {
    if integration_connection.is_connected() {
        if let Ok(source) = integration_connection.provider.kind().try_into() {
            notification_service.send(NotificationCommand::Sync(Some(source)));
        }
        if let Ok(source) = integration_connection.provider.kind().try_into() {
            task_service.send(TaskCommand::Sync(Some(source)));
        }
    }
}

fn get_configs(
    app_config: ReadSignal<Option<AppConfig>>,
    integration_provider_kind: IntegrationProviderKind,
) -> Result<(Url, NangoPublicKey, IntegrationProviderStaticConfig)> {
    if let Some(app_config) = app_config.read().as_ref() {
        Ok((
            app_config.nango_base_url.clone(),
            app_config.nango_public_key.clone(),
            app_config
                .integration_providers
                .get(&integration_provider_kind)
                .cloned()
                .context(format!(
                    "No provider config found for {integration_provider_kind}"
                ))?,
        ))
    } else {
        Err(anyhow!("Application not yet loaded, it is unexpected."))
    }
}

fn get_api_base_url(app_config: ReadSignal<Option<AppConfig>>) -> Result<Url> {
    if let Some(app_config) = app_config.read().as_ref() {
        Ok(app_config.api_base_url.clone())
    } else {
        Err(anyhow!("Application not yet loaded, it is unexpected."))
    }
}

#[allow(clippy::too_many_arguments)]
async fn update_integration_connection_config(
    integration_connection_id: IntegrationConnectionId,
    config: IntegrationConnectionConfig,
    integration_connections: Signal<Option<Vec<IntegrationConnection>>>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    app_config: ReadSignal<Option<AppConfig>>,
    ui_model: Signal<UniversalInboxUIModel>,
    notification_service: &Coroutine<NotificationCommand>,
    task_service: &Coroutine<TaskCommand>,
) -> Result<()> {
    let api_base_url = get_api_base_url(app_config)?;

    debug!("Updating integration connection {integration_connection_id} configuration: {config:?}");
    let _: IntegrationConnectionConfig = call_api(
        Method::PUT,
        &api_base_url,
        &format!("integration-connections/{integration_connection_id}/config"),
        Some(config),
        Some(ui_model),
    )
    .await?;

    refresh_integration_connection(
        integration_connections,
        task_service_integration_connection,
        app_config,
        ui_model,
        notification_service,
        task_service,
    )
    .await?;

    Ok(())
}
