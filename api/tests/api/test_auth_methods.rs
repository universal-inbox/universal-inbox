use std::collections::HashMap;

use rstest::*;

use universal_inbox::user::{UserAuthKind, UserAuthMethod, UserAuthMethodDisplayInfo};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app, get_all_user_auths},
    tested_app_with_domain_blacklist, tested_app_with_local_auth,
    user::{
        add_local_auth_response, finish_add_passkey_registration_response,
        link_oidc_pkce_session_response, list_auth_methods, list_auth_methods_response,
        register_user, remove_auth_method_response, start_add_passkey_registration_response,
    },
};

mod list_auth_methods {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_list_auth_methods_for_oidc_user(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        let methods = list_auth_methods(&app.client, &app.app).await;

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, UserAuthKind::OIDCAuthorizationCodePKCE);
        assert_eq!(
            methods[0].display_info,
            UserAuthMethodDisplayInfo::OIDCAuthorizationCodePKCE
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_auth_methods_for_local_user(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let (client, _user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let methods = list_auth_methods(&client, &app).await;

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, UserAuthKind::Local);
        assert_eq!(methods[0].display_info, UserAuthMethodDisplayInfo::Local);
    }

    #[rstest]
    #[tokio::test]
    async fn test_list_auth_methods_unauthenticated(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = list_auth_methods_response(&client, &app).await;

        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
    }
}

mod add_local_auth_method {
    use super::*;
    use crate::helpers::auth::authenticate_user;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_add_local_auth_to_oidc_user(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        // Authenticate via OIDC
        let (client, user) =
            authenticate_user(&app, "1234", "John", "Doe", "test@example.com").await;

        // Verify only OIDC auth exists
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, UserAuthKind::OIDCAuthorizationCodePKCE);

        // Add local auth
        let response = add_local_auth_response(&client, &app, "New-very-harD-pasSword-5").await;
        assert_eq!(response.status(), http::StatusCode::OK);

        let added_method: UserAuthMethod = response.json().await.unwrap();
        assert_eq!(added_method.kind, UserAuthKind::Local);
        assert_eq!(added_method.display_info, UserAuthMethodDisplayInfo::Local);

        // Verify both auth methods now exist
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 2);
        let kinds: Vec<UserAuthKind> = methods.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&UserAuthKind::Local));
        assert!(kinds.contains(&UserAuthKind::OIDCAuthorizationCodePKCE));

        // Verify in the database
        let user_auths = get_all_user_auths(&app, user.id).await;
        assert_eq!(user_auths.len(), 2);
    }

    #[rstest]
    #[tokio::test]
    async fn test_add_duplicate_local_auth(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (client, _user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Try to add local auth when user already has one
        let response = add_local_auth_response(&client, &app, "Another-pasSword-5").await;

        assert_eq!(response.status(), http::StatusCode::CONFLICT);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert!(body.get("message").unwrap().contains("already"));
    }
}

mod add_passkey_auth_method {
    use super::*;
    use crate::helpers::auth::get_all_user_auths;
    use pretty_assertions::assert_eq;
    use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
    use webauthn_rs::prelude::CreationChallengeResponse;

    #[rstest]
    #[tokio::test]
    async fn test_add_passkey_auth_to_local_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Verify only Local auth exists
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, UserAuthKind::Local);

        // Start passkey registration
        let response = start_add_passkey_registration_response(&client, &app, "john_passkey").await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let creation_challenge: CreationChallengeResponse = response.json().await.unwrap();

        // Simulate a software authenticator to complete the challenge
        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration with software authenticator");

        // Finish passkey registration
        let response =
            finish_add_passkey_registration_response(&client, &app, &register_credential).await;
        assert_eq!(response.status(), http::StatusCode::OK);

        let added_method: UserAuthMethod = response.json().await.unwrap();
        assert_eq!(added_method.kind, UserAuthKind::Passkey);
        assert_eq!(
            added_method.display_info,
            UserAuthMethodDisplayInfo::Passkey {
                username: "john_passkey".to_string()
            }
        );

        // Verify both auth methods now exist
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 2);
        let kinds: Vec<UserAuthKind> = methods.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&UserAuthKind::Local));
        assert!(kinds.contains(&UserAuthKind::Passkey));

        // Verify in the database
        let user_auths = get_all_user_auths(&app, user.id).await;
        assert_eq!(user_auths.len(), 2);
    }

    #[rstest]
    #[tokio::test]
    async fn test_add_passkey_auth_to_oidc_user(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        use crate::helpers::auth::authenticate_user;

        let app = tested_app_with_domain_blacklist.await;

        let (client, user) =
            authenticate_user(&app, "1234", "John", "Doe", "test@example.com").await;

        // Verify only OIDC auth exists
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, UserAuthKind::OIDCAuthorizationCodePKCE);

        // Start passkey registration
        let response = start_add_passkey_registration_response(&client, &app, "john_passkey").await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let creation_challenge: CreationChallengeResponse = response.json().await.unwrap();

        // Simulate a software authenticator to complete the challenge
        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration with software authenticator");

        // Finish passkey registration
        let response =
            finish_add_passkey_registration_response(&client, &app, &register_credential).await;
        assert_eq!(response.status(), http::StatusCode::OK);

        let added_method: UserAuthMethod = response.json().await.unwrap();
        assert_eq!(added_method.kind, UserAuthKind::Passkey);
        assert_eq!(
            added_method.display_info,
            UserAuthMethodDisplayInfo::Passkey {
                username: "john_passkey".to_string()
            }
        );

        // Verify both auth methods now exist
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 2);
        let kinds: Vec<UserAuthKind> = methods.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&UserAuthKind::OIDCAuthorizationCodePKCE));
        assert!(kinds.contains(&UserAuthKind::Passkey));

        // Verify in the database
        let user_auths = get_all_user_auths(&app, user.id).await;
        assert_eq!(user_auths.len(), 2);
    }
}

mod add_oidc_auth_method {
    use super::*;
    use crate::helpers::auth::{
        authenticate_user, get_all_user_auths, mock_oidc_introspection, mock_oidc_keys,
        mock_oidc_openid_configuration, mock_oidc_user_info,
    };
    use chrono::{TimeDelta, Utc};
    use openidconnect::{
        AccessToken, Audience, EmptyAdditionalClaims, EndUserEmail, IssuerUrl, StandardClaims,
        SubjectIdentifier,
        core::{CoreHmacKey, CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm},
    };
    use pretty_assertions::assert_eq;
    use universal_inbox::auth::SessionAuthValidationParameters;

    #[rstest]
    #[tokio::test]
    async fn test_add_oidc_pkce_auth_to_local_user(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        // Register a local user
        let (client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Verify only Local auth exists
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, UserAuthKind::Local);

        // Set up OIDC mocks for the linking flow
        mock_oidc_openid_configuration(&app).await;
        mock_oidc_keys(&app).await;
        mock_oidc_introspection(&app, "oidc-link-user-1234", true).await;
        mock_oidc_user_info(&app, "oidc-link-user-1234", "John", "Doe", "john@doe.name").await;

        // Create an id_token for the PKCE linking request
        let signing_key = CoreHmacKey::new("secret".as_bytes());
        let oidc_issuer_mock_server_url = app.oidc_issuer_mock_server.as_ref().unwrap().uri();
        let id_token = CoreIdToken::new(
            CoreIdTokenClaims::new(
                IssuerUrl::new(oidc_issuer_mock_server_url.to_string()).unwrap(),
                vec![Audience::new("john@doe.name-client-id-123".to_string())],
                Utc::now() + TimeDelta::try_seconds(120).unwrap(),
                Utc::now(),
                StandardClaims::new(SubjectIdentifier::new("John-Doe".to_string()))
                    .set_email(Some(EndUserEmail::new("john@doe.name".to_string()))),
                EmptyAdditionalClaims {},
            ),
            &signing_key,
            CoreJwsSigningAlgorithm::HmacSha256,
            None,
            None,
        )
        .unwrap();

        // Link OIDC PKCE auth to the local user
        let response = link_oidc_pkce_session_response(
            &client,
            &app,
            &SessionAuthValidationParameters {
                auth_id_token: id_token.to_string().into(),
                access_token: AccessToken::new("fake_token".to_string()),
            },
        )
        .await;
        assert_eq!(response.status(), http::StatusCode::OK);

        let added_method: UserAuthMethod = response.json().await.unwrap();
        assert_eq!(added_method.kind, UserAuthKind::OIDCAuthorizationCodePKCE);
        assert_eq!(
            added_method.display_info,
            UserAuthMethodDisplayInfo::OIDCAuthorizationCodePKCE
        );

        // Verify both auth methods now exist
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 2);
        let kinds: Vec<UserAuthKind> = methods.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&UserAuthKind::Local));
        assert!(kinds.contains(&UserAuthKind::OIDCAuthorizationCodePKCE));

        // Verify in the database
        let user_auths = get_all_user_auths(&app, user.id).await;
        assert_eq!(user_auths.len(), 2);
    }

    #[rstest]
    #[tokio::test]
    async fn test_add_duplicate_oidc_pkce_auth(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        // Authenticate via OIDC (user already has OIDC PKCE auth)
        let (client, _user) =
            authenticate_user(&app, "1234", "John", "Doe", "test@example.com").await;

        // Try to link another OIDC PKCE auth
        let signing_key = CoreHmacKey::new("secret".as_bytes());
        let oidc_issuer_mock_server_url = app.oidc_issuer_mock_server.as_ref().unwrap().uri();
        let id_token = CoreIdToken::new(
            CoreIdTokenClaims::new(
                IssuerUrl::new(oidc_issuer_mock_server_url.to_string()).unwrap(),
                vec![Audience::new("test@example.com-client-id-123".to_string())],
                Utc::now() + TimeDelta::try_seconds(120).unwrap(),
                Utc::now(),
                StandardClaims::new(SubjectIdentifier::new("John-Doe".to_string()))
                    .set_email(Some(EndUserEmail::new("test@example.com".to_string()))),
                EmptyAdditionalClaims {},
            ),
            &signing_key,
            CoreJwsSigningAlgorithm::HmacSha256,
            None,
            None,
        )
        .unwrap();

        let response = link_oidc_pkce_session_response(
            &client,
            &app,
            &SessionAuthValidationParameters {
                auth_id_token: id_token.to_string().into(),
                access_token: AccessToken::new("fake_token".to_string()),
            },
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::CONFLICT);
    }
}

mod add_google_auth_method {
    use super::*;
    use crate::helpers::auth::get_all_user_auths;
    use pretty_assertions::assert_eq;
    use universal_inbox_api::universal_inbox::user::model::{
        AuthUserId, OpenIdConnectUserAuth, UserAuth,
    };

    #[rstest]
    #[tokio::test]
    async fn test_add_google_auth_to_local_user(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        // Register a local user
        let (client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Verify only Local auth exists
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, UserAuthKind::Local);

        // Link Google OIDC auth via the service layer
        // (The full HTTP redirect flow requires mocking the OIDC token endpoint with
        // real RSA-signed id_tokens, so we use the service directly here)
        let mut transaction = app.repository.begin().await.unwrap();
        let auth_method = app
            .user_service
            .link_oidc_auth_method(
                &mut transaction,
                user.id,
                UserAuth::OIDCGoogleAuthorizationCode(Box::new(OpenIdConnectUserAuth {
                    auth_user_id: AuthUserId("google-user-12345".to_string()),
                    auth_id_token: "google-fake-id-token".to_string().into(),
                })),
                "john@doe.name".parse().unwrap(),
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        assert_eq!(auth_method.kind, UserAuthKind::OIDCGoogleAuthorizationCode);
        assert_eq!(
            auth_method.display_info,
            UserAuthMethodDisplayInfo::OIDCGoogleAuthorizationCode
        );

        // Verify both auth methods now exist via the HTTP API
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 2);
        let kinds: Vec<UserAuthKind> = methods.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&UserAuthKind::Local));
        assert!(kinds.contains(&UserAuthKind::OIDCGoogleAuthorizationCode));

        // Verify in the database
        let user_auths = get_all_user_auths(&app, user.id).await;
        assert_eq!(user_auths.len(), 2);
    }

    #[rstest]
    #[tokio::test]
    async fn test_add_duplicate_google_auth(#[future] tested_app_with_domain_blacklist: TestedApp) {
        use universal_inbox_api::universal_inbox::user::model::{
            AuthUserId, OpenIdConnectUserAuth, UserAuth,
        };

        let app = tested_app_with_domain_blacklist.await;

        // Create a local user and add Google auth
        let (_client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let mut transaction = app.repository.begin().await.unwrap();
        app.user_service
            .link_oidc_auth_method(
                &mut transaction,
                user.id,
                UserAuth::OIDCGoogleAuthorizationCode(Box::new(OpenIdConnectUserAuth {
                    auth_user_id: AuthUserId("google-user-12345".to_string()),
                    auth_id_token: "google-fake-id-token".to_string().into(),
                })),
                "john@doe.name".parse().unwrap(),
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        // Try to add Google auth again — should fail
        let mut transaction = app.repository.begin().await.unwrap();
        let result = app
            .user_service
            .link_oidc_auth_method(
                &mut transaction,
                user.id,
                UserAuth::OIDCGoogleAuthorizationCode(Box::new(OpenIdConnectUserAuth {
                    auth_user_id: AuthUserId("google-user-67890".to_string()),
                    auth_id_token: "google-fake-id-token-2".to_string().into(),
                })),
                "john@doe.name".parse().unwrap(),
            )
            .await;
        transaction.commit().await.unwrap();

        assert!(result.is_err());
    }
}

mod remove_auth_method {
    use super::*;
    use crate::helpers::auth::authenticate_user;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_remove_auth_method_with_multiple_methods(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        // Authenticate via OIDC and add local auth
        let (client, user) =
            authenticate_user(&app, "1234", "John", "Doe", "test@example.com").await;
        let response = add_local_auth_response(&client, &app, "Very-harD-pasSword-5").await;
        assert_eq!(response.status(), http::StatusCode::OK);

        // Verify two auth methods
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 2);

        // Remove local auth
        let response = remove_auth_method_response(&client, &app, UserAuthKind::Local).await;
        assert_eq!(response.status(), http::StatusCode::OK);

        // Verify only OIDC remains
        let methods = list_auth_methods(&client, &app).await;
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, UserAuthKind::OIDCAuthorizationCodePKCE);

        // Verify in the database
        let user_auths = get_all_user_auths(&app, user.id).await;
        assert_eq!(user_auths.len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn test_cannot_remove_last_auth_method(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        // Verify only one auth method
        let methods = list_auth_methods(&app.client, &app.app).await;
        assert_eq!(methods.len(), 1);

        // Try to remove the only auth method
        let response = remove_auth_method_response(
            &app.client,
            &app.app,
            UserAuthKind::OIDCAuthorizationCodePKCE,
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert!(body.get("message").unwrap().contains("last"));
    }
}
