use chrono::{TimeZone, Utc};
use httpmock::{
    Method::{GET, POST},
    MockServer,
};
use reqwest::Client;
use rstest::fixture;
use serde_json::json;

use super::{tested_app, TestedApp};

pub struct AuthenticatedApp {
    pub client: Client,
    pub app_address: String,
    pub github_mock_server: MockServer,
    pub todoist_mock_server: MockServer,
    pub oidc_issuer_mock_server: MockServer,
}

#[fixture]
pub async fn authenticated_app(#[future] tested_app: TestedApp) -> AuthenticatedApp {
    let app = tested_app.await;

    mock_oidc_openid_configuration(&app);
    mock_oidc_keys(&app);
    mock_oidc_introspection(&app, true);

    let client = Client::builder().cookie_store(true).build().unwrap();
    let auth_app = AuthenticatedApp {
        client,
        app_address: app.app_address.clone(),
        github_mock_server: app.github_mock_server,
        todoist_mock_server: app.todoist_mock_server,
        oidc_issuer_mock_server: app.oidc_issuer_mock_server,
    };

    let response = auth_app
        .client
        .get(&format!("{}/auth/session", app.app_address))
        .bearer_auth("fake_token")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    auth_app
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

pub fn mock_oidc_introspection(app: &TestedApp, active: bool) {
    let oidc_issuer_mock_server_uri = &app.oidc_issuer_mock_server.base_url();

    app.oidc_issuer_mock_server.mock(|when, then| {
        when.method(POST).path("/introspect");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "active": active,
                "sub": "subject",
                "scopes": "openid, profile, email",
                "client_id": "1234567890",
                "username": "test@example.com",
                "token_type": "Bearer",
                "exp": Utc.with_ymd_and_hms(2122, 1, 1, 0, 0, 0).unwrap().timestamp(),
                "iat": Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap().timestamp(),
                "nbf": Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap().timestamp(),
                "sub": "1234",
                "aud": ["1234567890"],
                "iss": &oidc_issuer_mock_server_uri,
                "jti": "1234567",
            }));
    });
}
