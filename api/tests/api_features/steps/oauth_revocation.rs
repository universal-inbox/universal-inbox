use cucumber::{given, then, when};
use pretty_assertions::assert_eq;

use universal_inbox::user::UserId;
use universal_inbox_api::universal_inbox::UniversalInboxError;

use crate::{
    helpers::oauth2_revocation::{
        Liveness, approve_existing_client, approved_client_with_code, authorization_codes,
        consent_again, consents, delete_consent, exchange, issue_code, refresh, refresh_tokens,
        revoke,
    },
    state::Exchange,
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

fn the_other_user(world: &ApiWorld) -> UserId {
    world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id
}

fn outcome(
    result: Result<universal_inbox::auth::oauth2::TokenResponse, UniversalInboxError>,
) -> Exchange {
    match result {
        Ok(_) => Exchange::Issued,
        Err(UniversalInboxError::InvalidInputData { user_error, .. }) => {
            Exchange::Refused(user_error)
        }
        Err(other) => panic!("The refusal is not an invalid request: {other:?}"),
    }
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

#[given(
    expr = "that other user also approved the client {string} and was issued an authorization code for it"
)]
async fn the_other_user_approved_the_client(world: &mut ApiWorld, name: String) {
    let user_id = the_other_user(world);
    let client_id = client_id(world, &name);
    let app = world.app.get().await;
    let code = approve_existing_client(app, user_id, &client_id).await;
    world.oauth_revocation.others_codes.insert(name, code);
}

#[given(expr = "that other user exchanged their authorization code of the client {string}")]
async fn the_other_user_exchanged(world: &mut ApiWorld, name: String) {
    let client_id = client_id(world, &name);
    let code = world.oauth_revocation.others_codes[&name].clone();
    let app = world.app.get().await;
    exchange(app, &code, &client_id)
        .await
        .expect("The other user could not exchange their code");
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
    let result = exchange(app, &code, &client_id).await;
    if let Ok(tokens) = &result {
        world
            .oauth_revocation
            .refresh_tokens
            .insert(name, tokens.refresh_token.clone());
    }
    world.oauth_revocation.exchange = Some(outcome(result));
}

#[given(expr = "that other user was issued another authorization code for the client {string}")]
async fn the_other_user_has_another_code(world: &mut ApiWorld, name: String) {
    let user_id = the_other_user(world);
    let client_id = client_id(world, &name);
    let app = world.app.get().await;
    let code = issue_code(app, user_id, &client_id).await;
    world.oauth_revocation.others_codes.insert(name, code);
}

#[when(expr = "that other user exchanges that other authorization code of the client {string}")]
async fn the_other_user_exchanges_it(world: &mut ApiWorld, name: String) {
    let client_id = client_id(world, &name);
    let code = world.oauth_revocation.others_codes[&name].clone();
    let app = world.app.get().await;
    world.oauth_revocation.exchange = Some(outcome(exchange(app, &code, &client_id).await));
}

#[when(expr = "I use the refresh token that the exchange of the client {string} gave me")]
async fn i_use_the_refresh_token(world: &mut ApiWorld, name: String) {
    let client_id = client_id(world, &name);
    let token = world.oauth_revocation.refresh_tokens[&name].clone();
    let app = world.app.get().await;
    world.oauth_revocation.exchange = Some(outcome(refresh(app, &token, &client_id).await));
}

#[when(
    expr = "{int} times in a row, I approve a new client, then revoke its authorization while exchanging its code at the same time, interleaved on the single-threaded scenario runtime"
)]
async fn i_race_revocation_and_exchange(world: &mut ApiWorld, times: usize) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut raced = Vec::new();
    let mut exchange_won = 0;
    for _ in 0..times {
        let (client_id, code) = approved_client_with_code(app, user_id).await;
        let (_, exchanged) = tokio::join!(
            revoke(app, user_id, &client_id),
            exchange(app, &code, &client_id)
        );
        if exchanged.is_ok() {
            exchange_won += 1;
        }
        raced.push(client_id);
    }
    world.oauth_revocation.raced_clients = raced;
    world.oauth_revocation.exchange_won = exchange_won;
}

#[then("the exchange issued tokens")]
fn the_exchange_issued_tokens(world: &mut ApiWorld) {
    assert_eq!(world.oauth_revocation.exchange, Some(Exchange::Issued));
}

#[then(expr = "the exchange was refused with {string}")]
fn the_exchange_was_refused(world: &mut ApiWorld, message: String) {
    assert_eq!(
        world.oauth_revocation.exchange,
        Some(Exchange::Refused(message))
    );
}

async fn counts_of(world: &mut ApiWorld, user_id: UserId, name: &str) -> ((i64, i64), (i64, i64)) {
    let client_id = client_id(world, name);
    let app = world.app.get().await;
    (
        (
            refresh_tokens(app, user_id, &client_id, Liveness::Any).await,
            refresh_tokens(app, user_id, &client_id, Liveness::Live).await,
        ),
        (
            authorization_codes(app, user_id, &client_id).await,
            consents(app, user_id, &client_id).await,
        ),
    )
}

#[then(expr = "the client {string} has {int} refresh token(s), {int} of them live")]
async fn the_client_has_refresh_tokens(world: &mut ApiWorld, name: String, all: i64, live: i64) {
    let user_id = world.user().id;
    assert_eq!(counts_of(world, user_id, &name).await.0, (all, live));
}

#[then(expr = "the client {string} has {int} authorization code(s) and {int} consent(s) of mine")]
async fn the_client_has_codes_and_consents(
    world: &mut ApiWorld,
    name: String,
    codes: i64,
    consents_count: i64,
) {
    let user_id = world.user().id;
    assert_eq!(
        counts_of(world, user_id, &name).await.1,
        (codes, consents_count)
    );
}

#[then(
    expr = "the client {string} has {int} refresh token(s), {int} of them live, {int} authorization code(s) and {int} consent(s) of that other user"
)]
async fn the_client_has_everything_of_the_other_user(
    world: &mut ApiWorld,
    name: String,
    all: i64,
    live: i64,
    codes: i64,
    consents_count: i64,
) {
    let user_id = the_other_user(world);
    assert_eq!(
        counts_of(world, user_id, &name).await,
        ((all, live), (codes, consents_count))
    );
}

#[then("none of those clients has a live refresh token")]
async fn none_has_a_live_refresh_token(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let raced = world.oauth_revocation.raced_clients.clone();
    let exchange_won = world.oauth_revocation.exchange_won;
    assert!(!raced.is_empty(), "No client was raced");
    let app = world.app.get().await;
    for client_id in &raced {
        assert_eq!(
            refresh_tokens(app, user_id, client_id, Liveness::Live).await,
            0,
            "a refresh token survived the revocation of {client_id} (the exchange succeeded in {exchange_won} of {} races)",
            raced.len()
        );
    }
}
