use std::sync::Arc;

use chrono::{TimeZone, Utc};
use httpmock::{
    Method::{GET, POST},
    MockServer,
};
use reqwest::Client;
use rstest::fixture;
use serde_json::json;

use universal_inbox::user::User;
use universal_inbox_api::repository::Repository;

use super::{tested_app, TestedApp};

pub struct AuthenticatedApp {
    pub client: Client,
    pub app_address: String,
    pub user: User,
    pub repository: Arc<Repository>,
    pub github_mock_server: MockServer,
    pub todoist_mock_server: MockServer,
    pub oidc_issuer_mock_server: MockServer,
    pub nango_mock_server: MockServer,
}

pub async fn authenticate_user(
    app: &TestedApp,
    auth_provider_user_id: &str,
    first_name: &str,
    last_name: &str,
    email: &str,
) -> (Client, User) {
    mock_oidc_openid_configuration(app);
    mock_oidc_keys(app);
    mock_oidc_introspection(app, auth_provider_user_id, true);
    mock_oidc_user_info(app, auth_provider_user_id, first_name, last_name, email);

    let client = Client::builder().cookie_store(true).build().unwrap();

    let response = client
        .get(&format!("{}/auth/session", app.app_address))
        .bearer_auth("fake_token")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let user: User = client
        .get(&format!("{}/auth/user", app.app_address))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    (client, user)
}

#[fixture]
pub async fn authenticated_app(#[future] tested_app: TestedApp) -> AuthenticatedApp {
    let app = tested_app.await;
    let (client, user) = authenticate_user(&app, "1234", "John", "Doe", "test@example.com").await;

    AuthenticatedApp {
        client,
        app_address: app.app_address.clone(),
        user,
        repository: app.repository.clone(),
        github_mock_server: app.github_mock_server,
        todoist_mock_server: app.todoist_mock_server,
        oidc_issuer_mock_server: app.oidc_issuer_mock_server,
        nango_mock_server: app.nango_mock_server,
    }
}

pub fn mock_oidc_openid_configuration(app: &TestedApp) {
    let oidc_issuer_mock_server_uri = &app.oidc_issuer_mock_server.base_url();

    app.oidc_issuer_mock_server.mock(|when, then| {
        when.method(GET).path("/.well-known/openid-configuration");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "authorization_endpoint": format!("{oidc_issuer_mock_server_uri}/authorize"),
                "jwks_uri": format!("{oidc_issuer_mock_server_uri}/keys"),
                "introspection_endpoint": format!("{oidc_issuer_mock_server_uri}/introspect"),
                "introspection_endpoint_auth_methods_supported": [
                    "client_secret_basic",
                    "private_key_jwt"
                ],
                "introspection_endpoint_auth_signing_alg_values_supported": [
                    "RS256"
                ],
                "issuer": &oidc_issuer_mock_server_uri,
                "response_types_supported": [
                    "code",
                    "id_token",
                    "id_token token"
                ],
                "subject_types_supported": [
                    "public"
                ],
                "id_token_signing_alg_values_supported": [
                    "RS256"
                ],
                "userinfo_endpoint": format!("{oidc_issuer_mock_server_uri}/userinfo")
            }));
    });
}

pub fn mock_oidc_keys(app: &TestedApp) {
    app.oidc_issuer_mock_server.mock(|when, then| {
        when.method(GET).path("/keys");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "keys": [
                    {
                        "alg": "RS256",
                        "e": "AAAA",
                        "kid": "12345",
                        "kty": "RSA",
                        "n": "xxxx",
                        "use": "sig"
                    },
                ]
            }));
    });
}

pub fn mock_oidc_introspection(app: &TestedApp, auth_provider_user_id: &str, active: bool) {
    let oidc_issuer_mock_server_uri = &app.oidc_issuer_mock_server.base_url();

    app.oidc_issuer_mock_server.mock(|when, then| {
        when.method(POST).path("/introspect");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "active": active,
                "scopes": "openid, profile, email",
                "client_id": "1234567890",
                "username": "test@example.com",
                "token_type": "Bearer",
                "exp": Utc.with_ymd_and_hms(2122, 1, 1, 0, 0, 0).unwrap().timestamp(),
                "iat": Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap().timestamp(),
                "nbf": Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap().timestamp(),
                "sub": auth_provider_user_id,
                "aud": ["1234567890"],
                "iss": &oidc_issuer_mock_server_uri,
                "jti": "1234567",
            }));
    });
}

pub fn mock_oidc_user_info(
    app: &TestedApp,
    auth_provider_user_id: &str,
    first_name: &str,
    last_name: &str,
    email: &str,
) {
    app.oidc_issuer_mock_server.mock(|when, then| {
        when.method(GET).path("/userinfo");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "sub": auth_provider_user_id,
                "name": format!("{} {}", first_name, last_name),
                "given_name": first_name,
                "family_name": last_name,
                "preferred_username": "username",
                "email": email,
            }));
    });
}
