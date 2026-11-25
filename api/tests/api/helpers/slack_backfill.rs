use universal_inbox::integration_connection::IntegrationConnectionId;

use crate::helpers::TestedApp;

pub async fn expire_the_access_token(app: &TestedApp, id: IntegrationConnectionId) {
    sqlx::query(
        "UPDATE oauth_credential SET access_token_expires_at = NOW() - INTERVAL '1 hour' WHERE integration_connection_id = $1",
    )
    .bind(id.0)
    .execute(&*app.repository.pool)
    .await
    .expect("Failed to expire the access token");
}
