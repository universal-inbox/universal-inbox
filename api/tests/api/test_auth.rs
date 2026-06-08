use std::time::SystemTime;

use openidconnect::AccessToken;
use rstest::*;

use universal_inbox::{
    auth::{AuthorizeSessionResponse, CloseSessionResponse, SessionAuthValidationParameters},
    user::{User, UserAuthKind},
};

use universal_inbox_api::{configuration::Settings, universal_inbox::user::model::UserAuth};

use crate::helpers::{
    TestedApp,
    auth::{
        AuthenticatedApp, authenticated_app, fetch_auth_tokens_for_user, get_user_auth,
        mock_oidc_introspection, mock_oidc_keys, mock_oidc_openid_configuration,
        mock_oidc_user_info,
    },
    settings, tested_app,
    user::logout_user_response,
};

mod authenticate_session {
    use super::*;

    const ID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjI0MTE5MDE1MjI5NzI1NDEyMSIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3Rlc3QteGJzYnMzLnppdGFkZWwuY2xvdWQiLCJzdWIiOiIxODE0MTE0MDYyODgwNjA2NzMiLCJhdWQiOlsiMjA1NjYyMjE0NDgzNDExMjAxQHVuaXZlcnNhbF9pbmJveCIsIjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCIyMDQzNTkzMDAyNTA4NjE4MjUiXSwiZXhwIjoxNzAwMjk5Nzc3LCJpYXQiOjE3MDAyNTY1NzcsImF1dGhfdGltZSI6MTY5NzcyMDU1Nywibm9uY2UiOiI0bk1obE01bm5xbXFLcXJKcjVqTkd3IiwiYW1yIjpbInBhc3N3b3JkIiwicHdkIl0sImF6cCI6IjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCJjbGllbnRfaWQiOiIyMDQzNTk1NjAwMjkzMzk5MDVAdW5pdmVyc2FsX2luYm94IiwiYXRfaGFzaCI6InJxMl81N3dacjJqNmlLY1dvZzhDNkEiLCJjX2hhc2giOiJUbE5jLXJzLVlkN2dHaVIwNkRjcGpBIn0.qoOPG0_Ia40xq0jzlOeMUtrxK5LjZhQJS3_RfUbtRZxXEGWd8krreN7J3qmIKHo_Xp8Ih5BZJon1GqSYUkdqjcVg-a8XNXE-1kqAqz2ViPbDGtmSfx8tl7ga_cIH2hXsYy1zNMxtdmCbCFaKGUt6XOs201gcx-2kyJLMvN0mcZ23W6VxcVuo9_CR_BXWFjc9WVw-Ws34UhWOxk0_sNRwpTg720KHOcmxXH118dKGhWNpFG9qJYbDaXuBJ1jwS4RTMbC5cruXfQiNAJ0aaeZM52yIno16YSN44_cpllRQgzoNIXF2i8GS7c2M2D1mEssilTI55t2W4VihahmrCUScZg";
    const OTHER_ID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjI0MTI3MDM2ODQ0NTIwNTczNyIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3Rlc3QteGJzYnMzLnppdGFkZWwuY2xvdWQiLCJzdWIiOiIxODE0MTE0MDYyODgwNjA2NzMiLCJhdWQiOlsiMjA1NjYyMjE0NDgzNDExMjAxQHVuaXZlcnNhbF9pbmJveCIsIjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCIyMDQzNTkzMDAyNTA4NjE4MjUiXSwiZXhwIjoxNzAwMzQ3NTUzLCJpYXQiOjE3MDAzMDQzNTMsImF1dGhfdGltZSI6MTY5NzcyMDU1Nywibm9uY2UiOiJ1R3NVdFNlWWVDS240dW4xdk9jZXRRIiwiYW1yIjpbInBhc3N3b3JkIiwicHdkIl0sImF6cCI6IjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCJjbGllbnRfaWQiOiIyMDQzNTk1NjAwMjkzMzk5MDVAdW5pdmVyc2FsX2luYm94IiwiYXRfaGFzaCI6IkRIaG81UFJZbkJqajNUeDdHVXljR3ciLCJjX2hhc2giOiJ6WmxtcWFvQmMwQ1ZFSl83Z2ZjbFNRIn0.a4cJLj6Fx1c2wcKxoU_fqBtTtbLpjxOaU8NE9UhnGxts2G0iXjm6N6duXu2yRSaxWV8hRYuQ8PJrl--EAC4wGnQ7zC2AwGjay8zll2zQR3ErR6pghUaNu_7Xr7yXSvysSspsSFBvc5cPQ1EITngxOExydtybiF0AJldwiLTfM_lMK-TsD118yLdhvOsofyY3n8397HIv3xpZHJsoPgGdLmgnT57TJP7krpL8fomTUuAZIj_5txk426mq4b5WcQ5Sxk-MZ3Zt3ktmD7jP5qHU6Xw4uwY9kxxkGSQZnTeucds_OlOUcU7daig_sm3XegJH69khvcZTfcNmwbTCfuYgWA";

    #[rstest]
    #[tokio::test]
    async fn test_authenticate_session_creation(#[future] tested_app: TestedApp) {
        let app = tested_app.await;

        mock_oidc_openid_configuration(&app).await;
        mock_oidc_keys(&app).await;
        mock_oidc_introspection(&app, "1234", true).await;
        mock_oidc_user_info(&app, "1234", "John", "Doe", "test@example.com").await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let response = client
            .post(format!("{}auth/session", app.api_address))
            .json(&SessionAuthValidationParameters {
                auth_id_token: ID_TOKEN.to_string().into(),
                access_token: AccessToken::new("fake_access_token".to_string()),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let user: User = client
            .get(format!("{}users/me", app.api_address))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(user.first_name, Some("John".to_string()));
        assert_eq!(user.last_name, Some("Doe".to_string()));
        let user_auth = get_user_auth(&app, user.id, UserAuthKind::OIDCAuthorizationCodePKCE).await;
        let UserAuth::OIDCAuthorizationCodePKCE(user_auth) = &user_auth else {
            panic!("User auth is not OIDCAuthorizationCodePKCE");
        };
        assert_eq!(user_auth.auth_id_token, ID_TOKEN.to_string().into());

        let auth_tokens = fetch_auth_tokens_for_user(&app, user.id).await;
        assert_eq!(auth_tokens.len(), 0);

        // Test a new ID token is updated
        let response = client
            .post(format!("{}auth/session", app.api_address))
            .json(&SessionAuthValidationParameters {
                auth_id_token: OTHER_ID_TOKEN.to_string().into(),
                access_token: AccessToken::new("fake_access_token".to_string()),
            })
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let user: User = client
            .get(format!("{}users/me", app.api_address))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let user_auth = get_user_auth(&app, user.id, UserAuthKind::OIDCAuthorizationCodePKCE).await;
        let UserAuth::OIDCAuthorizationCodePKCE(user_auth) = &user_auth else {
            panic!("User auth is not OIDCAuthorizationCodePKCE");
        };
        assert_eq!(user_auth.auth_id_token, OTHER_ID_TOKEN.to_string().into());
    }

    #[rstest]
    #[tokio::test]
    async fn test_authenticate_session_creation_wrong_access_token(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;

        mock_oidc_openid_configuration(&app).await;
        mock_oidc_keys(&app).await;
        mock_oidc_introspection(&app, "1234", false).await;

        let response = reqwest::Client::new()
            .post(format!("{}auth/session", app.api_address))
            .json(&SessionAuthValidationParameters {
                auth_id_token: ID_TOKEN.to_string().into(),
                access_token: AccessToken::new("fake_access_token".to_string()),
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
    async fn test_close_session(settings: Settings, #[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let oidc_issuer_mock_server_url = app.app.oidc_issuer_mock_server.as_ref().unwrap().uri();

        let response = logout_user_response(&app.client, &app.app.api_address).await;

        assert_eq!(response.status(), 200);
        let cookie_name = app.app.session_cookie_name();
        for cookie in response.cookies() {
            assert_eq!(cookie.name(), cookie_name.as_str());
            assert_eq!(cookie.value(), "");
            assert!(cookie.expires().unwrap() < SystemTime::now());
        }

        let close_session_response: CloseSessionResponse = response.json().await.unwrap();

        let user_auth = get_user_auth(
            &app.app,
            app.user.id,
            UserAuthKind::OIDCAuthorizationCodePKCE,
        )
        .await;
        let UserAuth::OIDCAuthorizationCodePKCE(user_auth) = user_auth else {
            panic!("User auth is not OIDCAuthorizationCodePKCE");
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

mod authorize_session {
    use super::*;

    /// Regression test for universal-inbox-bkj.15:
    /// `authorize_session` must clear the cached OIDC authorization URL so
    /// each login flow gets a fresh CSRF state. Otherwise, a user who started
    /// a link-OIDC flow then triggered `authorize_session` in another tab
    /// would have the OIDC callback reuse the cached CSRF state and silently
    /// fall through to the normal authentication branch — creating a new
    /// user instead of linking.
    #[rstest]
    #[tokio::test]
    async fn test_authorize_session_clears_cached_authorization_url(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;

        mock_oidc_openid_configuration(&app).await;
        mock_oidc_keys(&app).await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // First call: builds and caches a fresh OIDC authorization URL.
        let response1 = client
            .get(format!("{}auth/session/authorize", app.api_address))
            .send()
            .await
            .unwrap();
        assert_eq!(response1.status(), 200);
        let body1: AuthorizeSessionResponse = response1.json().await.unwrap();

        // Second call with the same session: must NOT return the cached URL.
        // Before the fix, `authorize_session` left the cached URL in place
        // and `build_oidc_authorization_response` returned it verbatim
        // (same CSRF state).
        let response2 = client
            .get(format!("{}auth/session/authorize", app.api_address))
            .send()
            .await
            .unwrap();
        assert_eq!(response2.status(), 200);
        let body2: AuthorizeSessionResponse = response2.json().await.unwrap();

        // The two URLs must differ: each call regenerates CsrfToken/Nonce.
        assert_ne!(
            body1.authorization_url, body2.authorization_url,
            "authorize_session returned the cached authorization URL; the \
             cached entry was not cleared and a stale CSRF state would be \
             reused by the OIDC callback (universal-inbox-bkj.15)"
        );

        // Sanity check: the CSRF `state` query param must differ between the
        // two URLs (this is the value that authenticated_session compares
        // against the IdP-provided state).
        let state1 = body1
            .authorization_url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned());
        let state2 = body2
            .authorization_url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned());
        assert!(state1.is_some(), "first authorization URL has no `state`");
        assert!(state2.is_some(), "second authorization URL has no `state`");
        assert_ne!(
            state1, state2,
            "authorize_session returned an authorization URL with the same \
             CSRF state across two calls — the cached URL was not cleared"
        );
    }
}
