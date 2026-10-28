//! Revoking an OAuth2 client authorization must kill outstanding authorization
//! codes and must never leave a live refresh token behind, even when the
//! revocation races a code exchange.

use base64::prelude::*;
use ring::digest;
use rstest::*;

use universal_inbox::user::UserId;
use universal_inbox_api::repository::oauth2::OAuth2Repository;

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
};

const REDIRECT_URI: &str = "https://client.example.test/cb";
const VERIFIER: &str = "vBe-rPMPbAt-pkce-verifier-with-enough-entropy-for-tests";

fn challenge() -> String {
    let digest = digest::digest(&digest::SHA256, VERIFIER.as_bytes());
    BASE64_URL_SAFE_NO_PAD.encode(digest.as_ref())
}

/// Register a client, store the user's consent and issue an authorization code,
/// mirroring what `/authorize` does once the user has approved.
async fn approved_client_with_code(app: &TestedApp, user_id: UserId) -> (String, String) {
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
    let code = app
        .oauth2_service
        .create_authorization_code(
            &mut tx,
            &client.client_id,
            user_id,
            REDIRECT_URI,
            Some("read"),
            &challenge(),
            "S256",
            None,
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (client.client_id, code)
}

async fn issue_code(app: &TestedApp, user_id: UserId, client_id: &str) -> String {
    let mut tx = app.oauth2_service.begin().await.unwrap();
    let code = app
        .oauth2_service
        .create_authorization_code(
            &mut tx,
            client_id,
            user_id,
            REDIRECT_URI,
            Some("read"),
            &challenge(),
            "S256",
            None,
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    code
}

async fn exchange(app: &TestedApp, code: &str, client_id: &str) -> bool {
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

async fn revoke(app: &TestedApp, user_id: UserId, client_id: &str) {
    let mut tx = app.oauth2_service.begin().await.unwrap();
    app.oauth2_service
        .revoke_client_authorization(&mut tx, user_id, client_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn count(app: &TestedApp, table: &str, user_id: UserId, client_id: &str, live: bool) -> i64 {
    let sql: &'static str = match (table, live) {
        ("oauth2_authorization_code", false) => {
            "SELECT COUNT(*) FROM oauth2_authorization_code WHERE user_id = $1 AND client_id = $2"
        }
        ("oauth2_user_consent", false) => {
            "SELECT COUNT(*) FROM oauth2_user_consent WHERE user_id = $1 AND client_id = $2"
        }
        ("oauth2_refresh_token", false) => {
            "SELECT COUNT(*) FROM oauth2_refresh_token WHERE user_id = $1 AND client_id = $2"
        }
        ("oauth2_refresh_token", true) => {
            "SELECT COUNT(*) FROM oauth2_refresh_token \
             WHERE user_id = $1 AND client_id = $2 AND revoked_at IS NULL"
        }
        _ => panic!("unsupported count query: {table} live={live}"),
    };
    sqlx::query_scalar(sql)
        .bind(user_id.0)
        .bind(client_id)
        .fetch_one(&*app.repository.pool)
        .await
        .unwrap()
}

#[rstest]
#[tokio::test]
async fn exchange_after_revocation_fails_and_issues_nothing(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let user_id = app.user.id;
    let (client_id, code) = approved_client_with_code(&app.app, user_id).await;

    revoke(&app.app, user_id, &client_id).await;

    assert!(!exchange(&app.app, &code, &client_id).await);
    assert_eq!(
        count(&app.app, "oauth2_refresh_token", user_id, &client_id, false).await,
        0
    );
}

#[rstest]
#[tokio::test]
async fn exchange_of_non_revoked_grant_still_works(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let user_id = app.user.id;
    let (client_id, code) = approved_client_with_code(&app.app, user_id).await;

    assert!(exchange(&app.app, &code, &client_id).await);
    assert_eq!(
        count(&app.app, "oauth2_refresh_token", user_id, &client_id, true).await,
        1
    );
}

#[rstest]
#[tokio::test]
async fn exchange_fails_when_consent_is_gone_even_if_code_survives(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let user_id = app.user.id;
    let (client_id, code) = approved_client_with_code(&app.app, user_id).await;

    let mut tx = app.app.oauth2_service.begin().await.unwrap();
    app.app
        .repository
        .delete_user_consent(&mut tx, user_id, &client_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert!(!exchange(&app.app, &code, &client_id).await);
    assert_eq!(
        count(&app.app, "oauth2_refresh_token", user_id, &client_id, false).await,
        0
    );
}

#[rstest]
#[tokio::test]
async fn revoking_one_client_leaves_other_client_untouched(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let user_id = app.user.id;
    let (client_a, _code_a) = approved_client_with_code(&app.app, user_id).await;
    let (client_b, code_b) = approved_client_with_code(&app.app, user_id).await;

    revoke(&app.app, user_id, &client_a).await;

    assert_eq!(
        count(
            &app.app,
            "oauth2_authorization_code",
            user_id,
            &client_a,
            false
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &app.app,
            "oauth2_authorization_code",
            user_id,
            &client_b,
            false
        )
        .await,
        1
    );
    assert_eq!(
        count(&app.app, "oauth2_user_consent", user_id, &client_b, false).await,
        1
    );
    assert!(exchange(&app.app, &code_b, &client_b).await);
}

#[rstest]
#[tokio::test]
async fn concurrent_revoke_and_exchange_never_leave_a_live_refresh_token(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let user_id = app.user.id;

    for _ in 0..20 {
        let (client_id, code) = approved_client_with_code(&app.app, user_id).await;

        let (_, _) = tokio::join!(
            revoke(&app.app, user_id, &client_id),
            exchange(&app.app, &code, &client_id)
        );

        assert_eq!(
            count(&app.app, "oauth2_refresh_token", user_id, &client_id, true).await,
            0,
            "a refresh token survived the revocation"
        );
    }
}

#[rstest]
#[tokio::test]
async fn codes_issued_after_revocation_and_reconsent_are_exchangeable(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let user_id = app.user.id;
    let (client_id, _code) = approved_client_with_code(&app.app, user_id).await;

    revoke(&app.app, user_id, &client_id).await;

    let mut tx = app.app.oauth2_service.begin().await.unwrap();
    app.app
        .repository
        .upsert_user_consent(&mut tx, user_id, &client_id, "read")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let new_code = issue_code(&app.app, user_id, &client_id).await;

    assert!(exchange(&app.app, &new_code, &client_id).await);
}
