use std::time::SystemTime;

use rstest::*;

use universal_inbox::{
    auth::{CloseSessionResponse, SessionAuthValidationParameters},
    user::User,
};

use universal_inbox_api::configuration::Settings;

use crate::helpers::{
    auth::{
        authenticated_app, mock_oidc_introspection, mock_oidc_keys, mock_oidc_openid_configuration,
        mock_oidc_user_info, AuthenticatedApp,
    },
    settings, tested_app, TestedApp,
};

mod authenticate_session {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_authenticate_session_creation(#[future] tested_app: TestedApp) {
        let app = tested_app.await;

        mock_oidc_openid_configuration(&app);
        mock_oidc_keys(&app);
        mock_oidc_introspection(&app, "1234", true);
        mock_oidc_user_info(&app, "1234", "John", "Doe", "test@example.com");

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let response = client
            .post(&format!("{}/auth/session", app.api_address))
            .bearer_auth("fake_access_token")
            .json(&SessionAuthValidationParameters {
                auth_id_token: "id token".to_string().into(),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let user: User = client
            .get(&format!("{}/auth/user", app.api_address))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(user.first_name, "John");
        assert_eq!(user.last_name, "Doe");
        assert_eq!(user.auth_id_token, "id token".to_string().into());

        // Test a new ID token is updated
        let response = client
            .post(&format!("{}/auth/session", app.api_address))
            .bearer_auth("fake_access_token")
            .json(&SessionAuthValidationParameters {
                auth_id_token: "other id token".to_string().into(),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let user: User = client
            .get(&format!("{}/auth/user", app.api_address))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(user.auth_id_token, "other id token".to_string().into());
    }

    #[rstest]
    #[tokio::test]
    async fn test_authenticate_session_creation_wrong_access_token(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;

        mock_oidc_openid_configuration(&app);
        mock_oidc_keys(&app);
        mock_oidc_introspection(&app, "1234", false);

        let response = reqwest::Client::new()
            .post(&format!("{}/auth/session", app.api_address))
            .bearer_auth("fake_access_token")
            .json(&SessionAuthValidationParameters {
                auth_id_token: "1234".to_string().into(),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 401);
    }
}

mod close_session {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_close_session(
        settings: Settings,
        #[future] tested_app: TestedApp,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let tested_app = tested_app.await;
        let oidc_issuer_mock_server_uri = &app.oidc_issuer_mock_server.base_url();

        mock_oidc_openid_configuration(&tested_app);

        let response = app
            .client
            .delete(&format!("{}/auth/session", app.api_address))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        for cookie in response.cookies() {
            assert_eq!(cookie.name(), "id");
            assert_eq!(cookie.value(), "");
            assert!(cookie.expires().unwrap() < SystemTime::now());
        }

        let close_session_response: CloseSessionResponse = response.json().await.unwrap();

        assert_eq!(
            close_session_response.logout_url.to_string(),
            format!(
                "{oidc_issuer_mock_server_uri}/end_session?{}",
                serde_urlencoded::to_string([
                    ("id_token_hint", app.user.auth_id_token.to_string()),
                    (
                        "post_logout_redirect_uri",
                        settings.application.front_base_url.to_string()
                    )
                ])
                .unwrap()
            )
        );
    }
}
