use base64::prelude::*;
use ring::digest;

use universal_inbox::user::UserId;
use universal_inbox_api::repository::oauth2::OAuth2Repository;

use crate::helpers::TestedApp;

const REDIRECT_URI: &str = "https://client.example.test/cb";
const VERIFIER: &str = "vBe-rPMPbAt-pkce-verifier-with-enough-entropy-for-tests";

pub enum Liveness {
    Any,
    Live,
}

fn challenge() -> String {
    let digest = digest::digest(&digest::SHA256, VERIFIER.as_bytes());
    BASE64_URL_SAFE_NO_PAD.encode(digest.as_ref())
}

/// Register a client, store the user's consent and issue an authorization code,
/// mirroring what `/authorize` does once the user has approved.
pub async fn approved_client_with_code(app: &TestedApp, user_id: UserId) -> (String, String) {
    let mut tx = app.oauth2_service.begin().await.unwrap();
    let client = app
        .oauth2_service
        .register_client(
            &mut tx,
            Some("Revocation test client".to_string()),
            vec![REDIRECT_URI.to_string()],
        )
        .await
        .unwrap();
    app.repository
        .upsert_user_consent(&mut tx, user_id, &client.client_id, "read")
        .await
        .unwrap();
    let code = create_code(app, &mut tx, user_id, &client.client_id).await;
    tx.commit().await.unwrap();
    (client.client_id, code)
}

async fn create_code(
    app: &TestedApp,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: UserId,
    client_id: &str,
) -> String {
    app.oauth2_service
        .create_authorization_code(
            tx,
            client_id,
            user_id,
            REDIRECT_URI,
            Some("read"),
            &challenge(),
            "S256",
            None,
        )
        .await
        .unwrap()
}

pub async fn consent_again(app: &TestedApp, user_id: UserId, client_id: &str) {
    let mut tx = app.oauth2_service.begin().await.unwrap();
    app.repository
        .upsert_user_consent(&mut tx, user_id, client_id, "read")
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

pub async fn issue_code(app: &TestedApp, user_id: UserId, client_id: &str) -> String {
    let mut tx = app.oauth2_service.begin().await.unwrap();
    let code = create_code(app, &mut tx, user_id, client_id).await;
    tx.commit().await.unwrap();
    code
}

pub async fn delete_consent(app: &TestedApp, user_id: UserId, client_id: &str) {
    let mut tx = app.oauth2_service.begin().await.unwrap();
    app.repository
        .delete_user_consent(&mut tx, user_id, client_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

pub async fn exchange(app: &TestedApp, code: &str, client_id: &str) -> bool {
    let mut tx = app.oauth2_service.begin().await.unwrap();
    match app
        .oauth2_service
        .exchange_code(&mut tx, code, client_id, REDIRECT_URI, VERIFIER)
        .await
    {
        Ok(_) => {
            tx.commit().await.unwrap();
            true
        }
        Err(_) => false,
    }
}

pub async fn revoke(app: &TestedApp, user_id: UserId, client_id: &str) {
    let mut tx = app.oauth2_service.begin().await.unwrap();
    app.oauth2_service
        .revoke_client_authorization(&mut tx, user_id, client_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

pub async fn authorization_codes(app: &TestedApp, user_id: UserId, client_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM oauth2_authorization_code WHERE user_id = $1 AND client_id = $2",
    )
    .bind(user_id.0)
    .bind(client_id)
    .fetch_one(&*app.repository.pool)
    .await
    .unwrap()
}

pub async fn consents(app: &TestedApp, user_id: UserId, client_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM oauth2_user_consent WHERE user_id = $1 AND client_id = $2",
    )
    .bind(user_id.0)
    .bind(client_id)
    .fetch_one(&*app.repository.pool)
    .await
    .unwrap()
}

pub async fn refresh_tokens(
    app: &TestedApp,
    user_id: UserId,
    client_id: &str,
    liveness: Liveness,
) -> i64 {
    let sql = match liveness {
        Liveness::Any => {
            "SELECT COUNT(*) FROM oauth2_refresh_token WHERE user_id = $1 AND client_id = $2"
        }
        Liveness::Live => {
            "SELECT COUNT(*) FROM oauth2_refresh_token \
             WHERE user_id = $1 AND client_id = $2 AND revoked_at IS NULL"
        }
    };
    sqlx::query_scalar(sql)
        .bind(user_id.0)
        .bind(client_id)
        .fetch_one(&*app.repository.pool)
        .await
        .unwrap()
}
