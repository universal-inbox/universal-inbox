use chrono::{TimeDelta, Utc};
use cucumber::{given, then};
use serde_json::json;

use crate::{
    common::settings,
    helpers::oauth2_cimd::{CIMD_URL, metadata_document, seed_cache_expiring_at},
    world::ApiWorld,
};

async fn publish(world: &mut ApiWorld, redirect_uri: String, valid_for: TimeDelta) {
    let doc = metadata_document(vec![redirect_uri]);
    let app = world.app.get().await;
    seed_cache_expiring_at(app, &doc, Utc::now() + valid_for).await;
    world.mcp_oauth.client = Some(json!({ "client_id": CIMD_URL }));
}

#[given(
    expr = "an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI {string} for the next 24 hours"
)]
async fn a_client_with_fresh_metadata(world: &mut ApiWorld, redirect_uri: String) {
    publish(world, redirect_uri, TimeDelta::hours(24)).await;
}

#[given(
    expr = "an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI {string} and expired an hour ago"
)]
async fn a_client_with_expired_metadata(world: &mut ApiWorld, redirect_uri: String) {
    publish(world, redirect_uri, TimeDelta::hours(-1)).await;
}

#[then(
    "the app redirects to the consent screen of the front end, for a new request and without a code"
)]
fn the_app_redirects_to_the_consent_screen(world: &mut ApiWorld) {
    let front = settings().application.front_base_url.to_string();
    let location = world
        .http
        .last()
        .header("location")
        .expect("The response has no location header")
        .to_string();
    let request_id = location
        .strip_prefix(&format!("{front}oauth2/consent?request_id="))
        .unwrap_or_else(|| panic!("{location:?} is not the consent screen of {front:?}"));
    assert!(
        !request_id.is_empty() && !request_id.contains('&'),
        "{location:?}"
    );
}
