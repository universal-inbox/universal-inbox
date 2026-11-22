use cucumber::given;

use crate::{
    helpers::auth::authenticate_user,
    world::{ApiWorld, Session},
};

#[given("I am signed in")]
async fn i_am_signed_in(world: &mut ApiWorld) {
    let app = world.app.get().await;
    assert!(
        app.oidc_issuer_mock_server.is_some(),
        "`I am signed in` signs in with the identity provider, which this app does not have"
    );
    let (client, user) = authenticate_user(app, "1234", "John", "Doe", "test@example.com").await;
    world.session = Some(Session { client, user });
}
