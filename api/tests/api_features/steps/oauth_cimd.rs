use chrono::{TimeDelta, Utc};
use cucumber::given;
use serde_json::json;

use crate::{
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
