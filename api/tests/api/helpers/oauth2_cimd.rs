use chrono::{DateTime, Utc};
use ring::digest;

use universal_inbox_api::{
    repository::oauth2::OAuth2Repository, universal_inbox::oauth2::cimd::ClientMetadataDocument,
};

use crate::helpers::TestedApp;

pub const CIMD_URL: &str = "https://cimd.example.test/client.json";

pub fn metadata_document(redirect_uris: Vec<String>) -> ClientMetadataDocument {
    ClientMetadataDocument {
        client_id: CIMD_URL.to_string(),
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

/// Pre-populating the cache lets `/authorize` accept a URL `client_id` without any outbound
/// CIMD fetch, which the SSRF guard would block for the loopback addresses of the tests.
pub async fn seed_cache_expiring_at(
    app: &TestedApp,
    doc: &ClientMetadataDocument,
    expires_at: DateTime<Utc>,
) {
    let mut tx = app
        .repository
        .begin()
        .await
        .expect("Begin tx for CIMD seed");
    let body_bytes = serde_json::to_vec(doc).unwrap();
    let body_hash = digest::digest(&digest::SHA256, &body_bytes)
        .as_ref()
        .to_vec();
    app.repository
        .upsert_cimd_metadata_cache(&mut tx, &doc.client_id, doc, &body_hash, expires_at)
        .await
        .expect("Seed CIMD cache");
    tx.commit().await.expect("Commit CIMD seed");
}
