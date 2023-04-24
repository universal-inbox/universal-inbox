use anyhow::{anyhow, Context, Result};
use dioxus::prelude::*;
use fermi::{AtomRef, UseAtomRef};
use futures_util::StreamExt;
use log::error;
use reqwest::Method;
use url::Url;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionCreation, IntegrationConnectionId,
        IntegrationConnectionStatus, IntegrationProviderKind,
    },
    IntegrationProviderConfig,
};

use crate::{
    components::toast_zone::{Toast, ToastKind},
    config::AppConfig,
    model::UniversalInboxUIModel,
    services::{api::call_api, nango::nango_auth, toast_service::ToastCommand},
};

#[derive(Debug)]
pub enum IntegrationConnectionCommand {
    Refresh,
    CreateIntegrationConnection(IntegrationProviderKind),
    AuthenticateIntegrationConnection(IntegrationConnection),
    DisconnectIntegrationConnection(IntegrationConnectionId),
    ReconnectIntegrationConnection(IntegrationConnection),
}

pub static INTEGRATION_CONNECTIONS: AtomRef<Vec<IntegrationConnection>> = |_| vec![];

pub async fn integration_connnection_service<'a>(
    mut rx: UnboundedReceiver<IntegrationConnectionCommand>,
    app_config_ref: UseAtomRef<Option<AppConfig>>,
    integration_connections_ref: UseAtomRef<Vec<IntegrationConnection>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(IntegrationConnectionCommand::Refresh) => {
                if let Err(error) = refresh_integration_connection(
                    &integration_connections_ref,
                    &app_config_ref,
                    &ui_model_ref,
                )
                .await
                {
                    error!("An error occurred while refreshing integration connections: {error:?}");
                    toast_service.send(ToastCommand::Push(Toast {
                        kind: ToastKind::Failure,
                        message: "An error occurred while refreshing integration connections. Please, retry 🙏 If the issue keeps happening, please contact our support.".to_string(),
                        timeout: Some(5_000),
                        ..Default::default()
                    }));
                }
            }
            Some(IntegrationConnectionCommand::CreateIntegrationConnection(
                integration_provider_kind,
            )) => {
                if let Err(error) = create_integration_connection(
                    integration_provider_kind,
                    &integration_connections_ref,
                    &app_config_ref,
                    &ui_model_ref,
                )
                .await
                {
                    error!("An error occurred while connecting with {integration_provider_kind}: {error:?}");
                    toast_service.send(ToastCommand::Push(Toast {
                        kind: ToastKind::Failure,
                        message: "An error occurred while connecting with {integration_provider_kind}. Please, retry 🙏 If the issue keeps happening, please contact our support.".to_string(),
                        timeout: Some(5_000),
                        ..Default::default()
                    }));
                }
            }
            Some(IntegrationConnectionCommand::AuthenticateIntegrationConnection(
                integration_connection,
            )) => {
                if let Err(error) = authenticate_integration_connection(
                    &integration_connection,
                    &integration_connections_ref,
                    &app_config_ref,
                    &ui_model_ref,
                )
                .await
                {
                    error!(
                        "An error occurred while authenticating with {}: {error:?}",
                        integration_connection.provider_kind
                    );
                    toast_service.send(ToastCommand::Push(Toast {
                        kind: ToastKind::Failure,
                        message: format!(
                            "An error occurred while authenticating with {}. Please, retry 🙏 If the issue keeps happening, please contact our support.",
                            integration_connection.provider_kind
                        ),
                        timeout: Some(5_000),
                        ..Default::default()
                    }));
                }
            }
            Some(IntegrationConnectionCommand::DisconnectIntegrationConnection(
                integration_connection_id,
            )) => {
                let _result = disconnect_integration_connection(
                    integration_connection_id,
                    &integration_connections_ref,
                    &app_config_ref,
                    &ui_model_ref,
                )
                .await;
            }
            Some(IntegrationConnectionCommand::ReconnectIntegrationConnection(
                integration_connection,
            )) => {
                if let Err(error) = reconnect_integration_connection(
                    &integration_connection,
                    &integration_connections_ref,
                    &app_config_ref,
                    &ui_model_ref,
                )
                .await
                {
                    error!(
                        "An error occurred while reconnecting with {}: {error:?}",
                        integration_connection.provider_kind
                    );
                    toast_service.send(ToastCommand::Push(Toast {
                        kind: ToastKind::Failure,
                        message: format!(
                            "An error occurred while reconnecting with {}. Please, retry 🙏 If the issue keeps happening, please contact our support.",
                            integration_connection.provider_kind
                        ),
                        timeout: Some(5_000),
                        ..Default::default()
                    }));
                }
            }
            None => {}
        }
    }
}

async fn refresh_integration_connection(
    integration_connections_ref: &UseAtomRef<Vec<IntegrationConnection>>,
    app_config_ref: &UseAtomRef<Option<AppConfig>>,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    let api_base_url = get_api_base_url(app_config_ref)?;

    let new_integration_connections: Vec<IntegrationConnection> = call_api(
        Method::GET,
        &api_base_url,
        "integration-connections",
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model_ref.clone()),
    )
    .await?;

    let mut integration_connections = integration_connections_ref.write();
    integration_connections.clear();
    integration_connections.extend(new_integration_connections);

    Ok(())
}

async fn create_integration_connection(
    integration_provider_kind: IntegrationProviderKind,
    integration_connections_ref: &UseAtomRef<Vec<IntegrationConnection>>,
    app_config_ref: &UseAtomRef<Option<AppConfig>>,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    let api_base_url = get_api_base_url(app_config_ref)?;

    let new_connection: IntegrationConnection = call_api(
        Method::POST,
        &api_base_url,
        "integration-connections",
        Some(IntegrationConnectionCreation {
            provider_kind: integration_provider_kind,
        }),
        Some(ui_model_ref.clone()),
    )
    .await?;

    {
        let mut integration_connections = integration_connections_ref.write();
        integration_connections.push(new_connection.clone());
    }

    authenticate_integration_connection(
        &new_connection,
        integration_connections_ref,
        app_config_ref,
        ui_model_ref,
    )
    .await
}

async fn authenticate_integration_connection(
    integration_connection: &IntegrationConnection,
    integration_connections_ref: &UseAtomRef<Vec<IntegrationConnection>>,
    app_config_ref: &UseAtomRef<Option<AppConfig>>,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    let (nango_base_url, provider_config) =
        get_configs(app_config_ref, integration_connection.provider_kind)?;

    nango_auth(
        &nango_base_url,
        &provider_config.nango_config_key,
        &integration_connection.connection_id,
    )
    .await?;

    verify_integration_connection(
        integration_connection.id,
        integration_connections_ref,
        app_config_ref,
        ui_model_ref,
    )
    .await
}

async fn verify_integration_connection(
    integration_connection_id: IntegrationConnectionId,
    integration_connections_ref: &UseAtomRef<Vec<IntegrationConnection>>,
    app_config_ref: &UseAtomRef<Option<AppConfig>>,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    let api_base_url = get_api_base_url(app_config_ref)?;

    let result: IntegrationConnection = call_api(
        Method::PATCH,
        &api_base_url,
        &format!("integration-connections/{integration_connection_id}/status"),
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model_ref.clone()),
    )
    .await?;

    update_integration_connection_status(
        result.id,
        result.status,
        result.failure_message,
        integration_connections_ref,
    );

    Ok(())
}

async fn disconnect_integration_connection(
    integration_connection_id: IntegrationConnectionId,
    integration_connections_ref: &UseAtomRef<Vec<IntegrationConnection>>,
    app_config_ref: &UseAtomRef<Option<AppConfig>>,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    let api_base_url = get_api_base_url(app_config_ref)?;

    let result: IntegrationConnection = call_api(
        Method::DELETE,
        &api_base_url,
        &format!("integration-connections/{integration_connection_id}"),
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model_ref.clone()),
    )
    .await?;

    update_integration_connection_status(
        result.id,
        result.status,
        result.failure_message,
        integration_connections_ref,
    );

    Ok(())
}

async fn reconnect_integration_connection(
    integration_connection: &IntegrationConnection,
    integration_connections_ref: &UseAtomRef<Vec<IntegrationConnection>>,
    app_config_ref: &UseAtomRef<Option<AppConfig>>,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    disconnect_integration_connection(
        integration_connection.id,
        integration_connections_ref,
        app_config_ref,
        ui_model_ref,
    )
    .await?;

    authenticate_integration_connection(
        integration_connection,
        integration_connections_ref,
        app_config_ref,
        ui_model_ref,
    )
    .await
}

fn update_integration_connection_status(
    integration_connection_id: IntegrationConnectionId,
    status: IntegrationConnectionStatus,
    failure_message: Option<String>,
    integration_connections_ref: &UseAtomRef<Vec<IntegrationConnection>>,
) {
    let mut integration_connections = integration_connections_ref.write();
    if let Some(integration_connection) = integration_connections
        .iter_mut()
        .find(|integration_connection| integration_connection.id == integration_connection_id)
    {
        integration_connection.status = status;
        integration_connection.failure_message = failure_message;
    }
}

fn get_configs(
    app_config_ref: &UseAtomRef<Option<AppConfig>>,
    integration_provider_kind: IntegrationProviderKind,
) -> Result<(Url, IntegrationProviderConfig)> {
    if let Some(app_config) = app_config_ref.read().as_ref() {
        Ok((
            app_config.nango_base_url.clone(),
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

fn get_api_base_url(app_config_ref: &UseAtomRef<Option<AppConfig>>) -> Result<Url> {
    if let Some(app_config) = app_config_ref.read().as_ref() {
        Ok(app_config.api_base_url.clone())
    } else {
        Err(anyhow!("Application not yet loaded, it is unexpected."))
    }
}
