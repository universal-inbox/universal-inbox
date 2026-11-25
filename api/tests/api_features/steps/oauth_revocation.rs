use cucumber::{given, then, when};
use pretty_assertions::assert_eq;

use crate::{
    helpers::oauth2_revocation::{
        Liveness, approved_client_with_code, authorization_codes, consent_again, consents,
        delete_consent, exchange, issue_code, refresh_tokens, revoke,
    },
    world::ApiWorld,
};

fn client_id(world: &ApiWorld, name: &str) -> String {
    world
        .oauth_revocation
        .clients
        .get(name)
        .unwrap_or_else(|| panic!("No OAuth2 client {name}"))
        .clone()
}

#[given(expr = "I approved the OAuth2 client {string} and was issued an authorization code for it")]
async fn i_approved_a_client(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let (client_id, code) = approved_client_with_code(app, user_id).await;
    world
        .oauth_revocation
        .clients
        .insert(name.clone(), client_id);
    world.oauth_revocation.codes.insert(name, code);
}

#[when(expr = "I revoke the authorization of the client {string}")]
async fn i_revoke(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let client_id = client_id(world, &name);
    let app = world.app.get().await;
    revoke(app, user_id, &client_id).await;
}

#[when(expr = "my consent for the client {string} is deleted, and its authorization code is kept")]
async fn my_consent_is_deleted(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let client_id = client_id(world, &name);
    let app = world.app.get().await;
    delete_consent(app, user_id, &client_id).await;
}

#[when(expr = "I approve the client {string} again, and am issued a new authorization code")]
async fn i_approve_again(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let client_id = client_id(world, &name);
    let app = world.app.get().await;
    consent_again(app, user_id, &client_id).await;
    let code = issue_code(app, user_id, &client_id).await;
    world.oauth_revocation.codes.insert(name, code);
}

#[when(expr = "I exchange the authorization code of the client {string}")]
async fn i_exchange(world: &mut ApiWorld, name: String) {
    let client_id = client_id(world, &name);
    let code = world.oauth_revocation.codes[&name].clone();
    let app = world.app.get().await;
    world.oauth_revocation.exchange_issued_tokens = Some(exchange(app, &code, &client_id).await);
}

#[when(
    expr = "{int} times in a row, I approve a new client, then revoke its authorization while exchanging its code at the same time"
)]
async fn i_race_revocation_and_exchange(world: &mut ApiWorld, times: usize) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut raced = Vec::new();
    for _ in 0..times {
        let (client_id, code) = approved_client_with_code(app, user_id).await;
        tokio::join!(
            revoke(app, user_id, &client_id),
            exchange(app, &code, &client_id)
        );
        raced.push(client_id);
    }
    world.oauth_revocation.raced_clients = raced;
}

#[then("the exchange issued tokens")]
fn the_exchange_issued_tokens(world: &mut ApiWorld) {
    assert_eq!(world.oauth_revocation.exchange_issued_tokens, Some(true));
}

#[then("the exchange was refused")]
fn the_exchange_was_refused(world: &mut ApiWorld) {
    assert_eq!(world.oauth_revocation.exchange_issued_tokens, Some(false));
}

#[then(expr = "the client {string} has {int} refresh token(s), {int} of them live")]
async fn the_client_has_refresh_tokens(world: &mut ApiWorld, name: String, all: i64, live: i64) {
    let user_id = world.user().id;
    let client_id = client_id(world, &name);
    let app = world.app.get().await;
    assert_eq!(
        (
            refresh_tokens(app, user_id, &client_id, Liveness::Any).await,
            refresh_tokens(app, user_id, &client_id, Liveness::Live).await
        ),
        (all, live)
    );
}

#[then(expr = "the client {string} has {int} authorization code(s) and {int} consent(s) of mine")]
async fn the_client_has_codes_and_consents(
    world: &mut ApiWorld,
    name: String,
    codes: i64,
    consents_count: i64,
) {
    let user_id = world.user().id;
    let client_id = client_id(world, &name);
    let app = world.app.get().await;
    assert_eq!(
        (
            authorization_codes(app, user_id, &client_id).await,
            consents(app, user_id, &client_id).await
        ),
        (codes, consents_count)
    );
}

#[then("none of those clients has a live refresh token")]
async fn none_has_a_live_refresh_token(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let raced = world.oauth_revocation.raced_clients.clone();
    assert!(!raced.is_empty(), "No client was raced");
    let app = world.app.get().await;
    for client_id in raced {
        assert_eq!(
            refresh_tokens(app, user_id, &client_id, Liveness::Live).await,
            0,
            "a refresh token survived the revocation of {client_id}"
        );
    }
}
