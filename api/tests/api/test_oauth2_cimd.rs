//! Integration tests for Client ID Metadata Discovery (CIMD).
//!
//! These tests exercise the `/authorize` and `/token` code paths against
//! `client_id` values that are URLs. To avoid the SSRF guard (which blocks
//! private/loopback IPs at resolve time), every test pre-populates the
//! `oauth2_client_metadata_cache` table via the repository. That way the
//! service's `resolve_cimd_client` hits the cache and never issues an HTTP
//! request. The fetcher itself is exercised by the unit tests inside
//! `api/src/universal_inbox/oauth2/cimd.rs`.

use base64::prelude::*;
use chrono::{TimeDelta, Utc};
use http::StatusCode;
use ring::digest;
use rstest::*;
use secrecy::ExposeSecret;
use serde_json::Value;

use universal_inbox::auth::auth_token::AuthenticationToken;
use universal_inbox_api::{
    repository::oauth2::OAuth2Repository, universal_inbox::oauth2::cimd::ClientMetadataDocument,
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
};

async fn create_api_key(app: &AuthenticatedApp) -> AuthenticationToken {
    app.client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .send()
        .await
        .expect("Failed to create API key")
        .json()
        .await
        .expect("Failed to deserialize API key response")
}

const TEST_CIMD_URL: &str = "https://cimd.example.test/client.json";
const TEST_REDIRECT_URI: &str = "https://cimd.example.test/cb";

fn make_doc(redirect_uris: Vec<String>) -> ClientMetadataDocument {
    ClientMetadataDocument {
        client_id: TEST_CIMD_URL.to_string(),
        client_name: Some("Test CIMD Client".to_string()),
        redirect_uris,
        grant_types: vec![
            "authorization_code".to_string(),
            "refresh_token".to_string(),
        ],
        response_types: vec!["code".to_string()],
        token_endpoint_auth_method: Some("none".to_string()),
        scope: Some("read write".to_string()),
        client_uri: None,
        logo_uri: None,
        tos_uri: None,
        policy_uri: None,
        software_id: None,
        software_version: None,
        application_type: Some("web".to_string()),
        jwks_uri: None,
        jwks: None,
    }
}

async fn seed_cimd_cache_at(
    app: &TestedApp,
    doc: &ClientMetadataDocument,
    expires_at: chrono::DateTime<Utc>,
) {
    let mut tx = app
        .repository
        .begin()
        .await
        .expect("Begin tx for CIMD seed");
    let body_bytes = serde_json::to_vec(&doc).unwrap();
    let body_hash = digest::digest(&digest::SHA256, &body_bytes)
        .as_ref()
        .to_vec();
    app.repository
        .upsert_cimd_metadata_cache(&mut tx, &doc.client_id, doc, &body_hash, expires_at)
        .await
        .expect("Seed CIMD cache");
    tx.commit().await.expect("Commit CIMD seed");
}

fn pkce_verifier() -> &'static str {
    "vBe-rPMPbAt-pkce-verifier-with-enough-entropy-for-tests"
}

fn pkce_challenge() -> String {
    let digest = digest::digest(&digest::SHA256, pkce_verifier().as_bytes());
    BASE64_URL_SAFE_NO_PAD.encode(digest.as_ref())
}

fn build_authorize_url(api_address: &str, client_id: &str, redirect_uri: &str) -> String {
    format!(
        "{}oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope=read+write&state=s",
        api_address,
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        pkce_challenge(),
    )
}

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap()
}

/// Pre-populating the cache lets `/authorize` accept a URL `client_id` and
/// reach the consent flow without any outbound CIMD fetch.
#[rstest]
#[tokio::test]
async fn cimd_authorize_resolves_client_from_cache(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let doc = make_doc(vec![TEST_REDIRECT_URI.to_string()]);
    seed_cimd_cache_at(&app.app, &doc, Utc::now() + TimeDelta::hours(24)).await;

    let api_key = create_api_key(&app).await;
    let token = api_key.jwt_token.expose_secret().0.clone();

    let response = no_redirect_client()
        .get(build_authorize_url(
            &app.app.api_address,
            TEST_CIMD_URL,
            TEST_REDIRECT_URI,
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("/authorize");

    // `/authorize` redirects (302) to the consent screen — the point of this
    // test is that the URL `client_id` was accepted and reached
    // `resolve_client`, not that the full consent flow succeeded (covered by
    // the existing DCR tests in test_mcp.rs).
    assert_eq!(
        response.status(),
        StatusCode::FOUND,
        "Expected /authorize to accept CIMD client_id and redirect to consent"
    );
    let location = response
        .headers()
        .get("location")
        .expect("missing location header")
        .to_str()
        .unwrap();
    assert!(
        location.contains("/oauth2/consent?request_id="),
        "Expected consent redirect, got: {location}"
    );
}

/// A redirect_uri not declared in the cached CIMD document must be rejected
/// at `/authorize` — same enforcement as for DCR-registered clients.
#[rstest]
#[tokio::test]
async fn cimd_authorize_rejects_redirect_uri_not_in_doc(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let doc = make_doc(vec![TEST_REDIRECT_URI.to_string()]);
    seed_cimd_cache_at(&app.app, &doc, Utc::now() + TimeDelta::hours(24)).await;

    let api_key = create_api_key(&app).await;
    let token = api_key.jwt_token.expose_secret().0.clone();

    let response = no_redirect_client()
        .get(build_authorize_url(
            &app.app.api_address,
            TEST_CIMD_URL,
            "https://attacker.example/steal",
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("/authorize");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// An expired cache row must trigger a refetch — and since the CIMD URL
/// `cimd.example.test` doesn't resolve, the refetch fails. The AS must
/// surface that error instead of silently serving the stale doc.
#[rstest]
#[tokio::test]
async fn cimd_authorize_refetches_after_cache_expiry(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let doc = make_doc(vec![TEST_REDIRECT_URI.to_string()]);
    seed_cimd_cache_at(&app.app, &doc, Utc::now() - TimeDelta::hours(1)).await;

    let api_key = create_api_key(&app).await;
    let token = api_key.jwt_token.expose_secret().0.clone();

    let response = no_redirect_client()
        .get(build_authorize_url(
            &app.app.api_address,
            TEST_CIMD_URL,
            TEST_REDIRECT_URI,
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("/authorize");

    assert!(
        response.status().is_client_error() || response.status().is_server_error(),
        "Expired CIMD cache must trigger refetch, not serve stale doc (got {})",
        response.status()
    );
}

/// The AS advertises CIMD support in its RFC 8414 metadata, and the field
/// name follows the IETF draft.
#[rstest]
#[tokio::test]
async fn well_known_advertises_cimd(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let response = no_redirect_client()
        .get(format!(
            "{}/.well-known/oauth-authorization-server",
            app.app.app_address.trim_end_matches('/')
        ))
        .send()
        .await
        .expect("Failed to fetch authorization server metadata");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["client_id_metadata_document_supported"], true);
}
