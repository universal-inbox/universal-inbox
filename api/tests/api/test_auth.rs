use std::time::SystemTime;

use rstest::*;

use universal_inbox::{
    auth::{CloseSessionResponse, SessionAuthValidationParameters},
    user::{User, UserAuth},
};

use universal_inbox_api::configuration::Settings;

use crate::helpers::{
    auth::{
        authenticated_app, mock_oidc_introspection, mock_oidc_keys, mock_oidc_openid_configuration,
        mock_oidc_user_info, AuthenticatedApp,
    },
    settings, tested_app,
    user::logout_user_response,
    TestedApp,
};

mod authenticate_session {
    use super::*;

    const ID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjI0MTE5MDE1MjI5NzI1NDEyMSIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3Rlc3QteGJzYnMzLnppdGFkZWwuY2xvdWQiLCJzdWIiOiIxODE0MTE0MDYyODgwNjA2NzMiLCJhdWQiOlsiMjA1NjYyMjE0NDgzNDExMjAxQHVuaXZlcnNhbF9pbmJveCIsIjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCIyMDQzNTkzMDAyNTA4NjE4MjUiXSwiZXhwIjoxNzAwMjk5Nzc3LCJpYXQiOjE3MDAyNTY1NzcsImF1dGhfdGltZSI6MTY5NzcyMDU1Nywibm9uY2UiOiI0bk1obE01bm5xbXFLcXJKcjVqTkd3IiwiYW1yIjpbInBhc3N3b3JkIiwicHdkIl0sImF6cCI6IjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCJjbGllbnRfaWQiOiIyMDQzNTk1NjAwMjkzMzk5MDVAdW5pdmVyc2FsX2luYm94IiwiYXRfaGFzaCI6InJxMl81N3dacjJqNmlLY1dvZzhDNkEiLCJjX2hhc2giOiJUbE5jLXJzLVlkN2dHaVIwNkRjcGpBIn0.qoOPG0_Ia40xq0jzlOeMUtrxK5LjZhQJS3_RfUbtRZxXEGWd8krreN7J3qmIKHo_Xp8Ih5BZJon1GqSYUkdqjcVg-a8XNXE-1kqAqz2ViPbDGtmSfx8tl7ga_cIH2hXsYy1zNMxtdmCbCFaKGUt6XOs201gcx-2kyJLMvN0mcZ23W6VxcVuo9_CR_BXWFjc9WVw-Ws34UhWOxk0_sNRwpTg720KHOcmxXH118dKGhWNpFG9qJYbDaXuBJ1jwS4RTMbC5cruXfQiNAJ0aaeZM52yIno16YSN44_cpllRQgzoNIXF2i8GS7c2M2D1mEssilTI55t2W4VihahmrCUScZg";
    const OTHER_ID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjI0MTI3MDM2ODQ0NTIwNTczNyIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3Rlc3QteGJzYnMzLnppdGFkZWwuY2xvdWQiLCJzdWIiOiIxODE0MTE0MDYyODgwNjA2NzMiLCJhdWQiOlsiMjA1NjYyMjE0NDgzNDExMjAxQHVuaXZlcnNhbF9pbmJveCIsIjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCIyMDQzNTkzMDAyNTA4NjE4MjUiXSwiZXhwIjoxNzAwMzQ3NTUzLCJpYXQiOjE3MDAzMDQzNTMsImF1dGhfdGltZSI6MTY5NzcyMDU1Nywibm9uY2UiOiJ1R3NVdFNlWWVDS240dW4xdk9jZXRRIiwiYW1yIjpbInBhc3N3b3JkIiwicHdkIl0sImF6cCI6IjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCJjbGllbnRfaWQiOiIyMDQzNTk1NjAwMjkzMzk5MDVAdW5pdmVyc2FsX2luYm94IiwiYXRfaGFzaCI6IkRIaG81UFJZbkJqajNUeDdHVXljR3ciLCJjX2hhc2giOiJ6WmxtcWFvQmMwQ1ZFSl83Z2ZjbFNRIn0.a4cJLj6Fx1c2wcKxoU_fqBtTtbLpjxOaU8NE9UhnGxts2G0iXjm6N6duXu2yRSaxWV8hRYuQ8PJrl--EAC4wGnQ7zC2AwGjay8zll2zQR3ErR6pghUaNu_7Xr7yXSvysSspsSFBvc5cPQ1EITngxOExydtybiF0AJldwiLTfM_lMK-TsD118yLdhvOsofyY3n8397HIv3xpZHJsoPgGdLmgnT57TJP7krpL8fomTUuAZIj_5txk426mq4b5WcQ5Sxk-MZ3Zt3ktmD7jP5qHU6Xw4uwY9kxxkGSQZnTeucds_OlOUcU7daig_sm3XegJH69khvcZTfcNmwbTCfuYgWA";

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
            .post(&format!("{}auth/session", app.api_address))
            .bearer_auth("fake_access_token")
            .json(&SessionAuthValidationParameters {
                auth_id_token: ID_TOKEN.to_string().into(),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let user: User = client
            .get(&format!("{}users/me", app.api_address))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(user.first_name, "John");
        assert_eq!(user.last_name, "Doe");
        let UserAuth::OpenIdConnect(user_auth) = &user.auth else {
            panic!("User auth is not OpenIdConnect");
        };
        assert_eq!(user_auth.auth_id_token, ID_TOKEN.to_string().into());

        // Test a new ID token is updated
        let response = client
            .post(&format!("{}auth/session", app.api_address))
            .bearer_auth("fake_access_token")
            .json(&SessionAuthValidationParameters {
                auth_id_token: OTHER_ID_TOKEN.to_string().into(),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let user: User = client
            .get(&format!("{}users/me", app.api_address))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let UserAuth::OpenIdConnect(user_auth) = &user.auth else {
            panic!("User auth is not OpenIdConnect");
        };
        assert_eq!(user_auth.auth_id_token, OTHER_ID_TOKEN.to_string().into());
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
            .post(&format!("{}auth/session", app.api_address))
            .bearer_auth("fake_access_token")
            .json(&SessionAuthValidationParameters {
                auth_id_token: ID_TOKEN.to_string().into(),
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
        let oidc_issuer_mock_server_url = app.oidc_issuer_mock_server.as_ref().unwrap().base_url();

        mock_oidc_openid_configuration(&tested_app);

        let response = logout_user_response(&app.client, &app.api_address).await;

        assert_eq!(response.status(), 200);
        for cookie in response.cookies() {
            assert_eq!(cookie.name(), "id");
            assert_eq!(cookie.value(), "");
            assert!(cookie.expires().unwrap() < SystemTime::now());
        }

        let close_session_response: CloseSessionResponse = response.json().await.unwrap();

        let UserAuth::OpenIdConnect(user_auth) = &app.user.auth else {
            panic!("User auth is not OpenIdConnect");
        };
        assert_eq!(
            close_session_response.logout_url.to_string(),
            format!(
                "{oidc_issuer_mock_server_url}/end_session?{}",
                serde_urlencoded::to_string([
                    ("id_token_hint", user_auth.auth_id_token.to_string()),
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
