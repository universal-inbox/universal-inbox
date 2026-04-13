use std::sync::Arc;

use actix_jwt_authc::Authenticated;
use actix_web::{HttpResponse, Scope, web};
use anyhow::Context;
use serde::Deserialize;

use universal_inbox::{
    integration_connection::integrations::slack::SlackExtensionCredential,
    slack_bridge::SlackBridgePendingActionId, user::UserId,
};

use crate::universal_inbox::{UniversalInboxError, slack_bridge::service::SlackBridgeService};

use super::super::utils::jwt::Claims;

pub fn scope() -> Scope {
    web::scope("/slack-bridge")
        .route("/pending-actions", web::post().to(get_pending_actions))
        .route(
            "/actions/{action_id}/complete",
            web::post().to(complete_action),
        )
        .route("/actions/{action_id}/fail", web::post().to(fail_action))
        .route("/status", web::get().to(get_bridge_status))
}

#[derive(Debug, Deserialize)]
pub struct PendingActionsRequest {
    pub credentials: Vec<SlackExtensionCredential>,
}

pub async fn get_pending_actions(
    body: web::Json<PendingActionsRequest>,
    slack_bridge_service: web::Data<Arc<SlackBridgeService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let mut transaction = slack_bridge_service.begin().await?;

    let actions = slack_bridge_service
        .get_actionable_actions_for_extension(
            &mut transaction,
            user_id,
            body.into_inner().credentials,
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&actions).context("Failed to serialize response")?))
}

#[derive(Debug, Deserialize)]
pub struct ActionPath {
    action_id: SlackBridgePendingActionId,
}

pub async fn complete_action(
    path: web::Path<ActionPath>,
    slack_bridge_service: web::Data<Arc<SlackBridgeService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let mut transaction = slack_bridge_service.begin().await?;

    slack_bridge_service
        .complete_action(&mut transaction, path.action_id, user_id)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction")?;

    Ok(HttpResponse::Ok().finish())
}

#[derive(Debug, Deserialize)]
pub struct FailActionBody {
    error: String,
}

pub async fn fail_action(
    path: web::Path<ActionPath>,
    body: web::Json<FailActionBody>,
    slack_bridge_service: web::Data<Arc<SlackBridgeService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let mut transaction = slack_bridge_service.begin().await?;

    slack_bridge_service
        .fail_action(&mut transaction, path.action_id, user_id, &body.error)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction")?;

    Ok(HttpResponse::Ok().finish())
}

pub async fn get_bridge_status(
    slack_bridge_service: web::Data<Arc<SlackBridgeService>>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let mut transaction = slack_bridge_service.begin().await?;

    let status = slack_bridge_service
        .get_bridge_status(&mut transaction, user_id)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&status).context("Failed to serialize response")?))
}
