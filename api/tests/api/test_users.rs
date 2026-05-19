use std::{collections::HashMap, str::FromStr};

use email_address::EmailAddress;
use itertools::Itertools;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    auth::auth_token::AuthenticationToken,
    user::{
        EmailValidationToken, Password, PasswordResetToken, User, UserAuthKind, UserId, UserPatch,
    },
};

use universal_inbox_api::{
    configuration::Settings, mailer::EmailTemplate, universal_inbox::user::model::UserAuth,
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app, fetch_auth_tokens_for_user, get_user_auth},
    settings, tested_app_with_local_auth,
    user::{
        get_current_user, get_current_user_response, get_password_reset_token,
        get_user_email_validation_token, login_user_response, logout_user_response,
        patch_user_response, register_user, register_user_response,
    },
};

mod register_user {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_register_user(
        settings: Settings,
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let (_client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        assert_eq!(user.email, Some("john@doe.name".parse().unwrap()));
        assert!(user.email_validated_at.is_none());
        assert!(!user.is_email_validated());
        assert!(user.email_validation_sent_at.is_some());

        let auth_tokens = fetch_auth_tokens_for_user(&app, user.id).await;
        assert_eq!(auth_tokens.len(), 0);

        let email_validation_token = get_user_email_validation_token(&app, user.id).await;

        assert!(email_validation_token.is_some());

        let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
        assert_eq!(emails_sent.len(), 1);
        assert_eq!(emails_sent[0].0.id, user.id);
        assert_eq!(
            emails_sent[0].1,
            EmailTemplate::EmailVerification {
                first_name: None,
                email_verification_url: format!(
                    "{}users/{}/email-verification/{}",
                    settings.application.front_base_url,
                    user.id,
                    email_validation_token.unwrap()
                )
                .parse()
                .unwrap()
            }
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_register_existing_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (client, _user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let response = register_user_response(
            &client,
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            "Unauthorized access: A user with this email address already exists"
        );
    }
}

mod email_domain_blacklist {
    use super::*;
    use crate::helpers::{
        auth::{
            mock_oidc_introspection, mock_oidc_keys, mock_oidc_openid_configuration,
            mock_oidc_user_info,
        },
        tested_app_with_domain_blacklist,
    };
    use openidconnect::AccessToken;
    use pretty_assertions::assert_eq;
    use universal_inbox::auth::SessionAuthValidationParameters;

    #[rstest]
    #[tokio::test]
    async fn test_register_user_with_blacklisted_domain(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = register_user_response(
            &client,
            &app,
            "user@blocked.com".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            "Forbidden access: Registration is not allowed from this domain"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_register_user_with_allowed_domain(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        let (_client, user) = register_user(
            &app,
            "user@allowed.com".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        assert_eq!(user.email, Some("user@allowed.com".parse().unwrap()));
    }

    #[rstest]
    #[tokio::test]
    async fn test_register_user_blacklist_case_insensitive(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = register_user_response(
            &client,
            &app,
            "user@BLOCKED.COM".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            "Forbidden access: Registration is not allowed from this domain"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_oidc_authenticate_with_blacklisted_domain(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        use chrono::{TimeDelta, Utc};
        use openidconnect::{
            Audience, EmptyAdditionalClaims, EndUserEmail, IssuerUrl, StandardClaims,
            SubjectIdentifier,
            core::{CoreHmacKey, CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm},
        };

        let app = tested_app_with_domain_blacklist.await;

        // Set up OIDC mocks with a blacklisted domain email
        app.oidc_issuer_mock_server.as_ref().unwrap().reset().await;
        mock_oidc_openid_configuration(&app).await;
        mock_oidc_keys(&app).await;
        mock_oidc_introspection(&app, "1234", true).await;
        mock_oidc_user_info(&app, "1234", "John", "Doe", "user@blocked.com").await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Create an ID token
        let signing_key = CoreHmacKey::new("secret".as_bytes());
        let oidc_issuer_mock_server_url = app
            .oidc_issuer_mock_server
            .as_ref()
            .map(|s| s.uri())
            .unwrap();
        let id_token = CoreIdToken::new(
            CoreIdTokenClaims::new(
                IssuerUrl::new(oidc_issuer_mock_server_url.to_string()).unwrap(),
                vec![Audience::new("user@blocked.com-client-id-123".to_string())],
                Utc::now() + TimeDelta::try_seconds(120).unwrap(),
                Utc::now(),
                StandardClaims::new(SubjectIdentifier::new("John-Doe".to_string()))
                    .set_email(Some(EndUserEmail::new("user@blocked.com".to_string()))),
                EmptyAdditionalClaims {},
            ),
            &signing_key,
            CoreJwsSigningAlgorithm::HmacSha256,
            None,
            None,
        )
        .unwrap();

        // Try to authenticate via OIDC
        let response = client
            .post(format!("{}auth/session", app.api_address))
            .json(&SessionAuthValidationParameters {
                auth_id_token: id_token.to_string().into(),
                access_token: AccessToken::new("fake_token".to_string()),
            })
            .send()
            .await
            .unwrap();

        // Should be forbidden due to blacklisted domain
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            "Forbidden access: Registration is not allowed from this domain"
        );
    }
}

mod login_user {
    use std::time::SystemTime;

    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_login_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (_client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let auth_tokens = fetch_auth_tokens_for_user(&app, user.id).await;
        assert_eq!(auth_tokens.len(), 0);

        // Create a new client to avoid using the same session
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = get_current_user_response(&client, &app).await;

        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
        // Cookies are reset on unauthorized access in case of malformed cookies
        for cookie in response.cookies() {
            assert_eq!(cookie.name(), "id");
            assert_eq!(cookie.value(), "");
            assert!(cookie.expires().unwrap() < SystemTime::now());
        }

        let login_response = login_user_response(
            &client,
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        assert_eq!(login_response.status(), http::StatusCode::OK);
        let logged_user: User = login_response.json().await.unwrap();
        assert_eq!(logged_user.id, user.id);

        let auth_tokens = fetch_auth_tokens_for_user(&app, logged_user.id).await;
        assert_eq!(auth_tokens.len(), 0);

        let user = get_current_user(&client, &app).await;

        assert_eq!(user.email, Some("john@doe.name".parse().unwrap()));
        assert!(user.email_validated_at.is_none());
        assert!(user.email_validation_sent_at.is_some());
    }

    #[rstest]
    #[tokio::test]
    async fn test_login_with_wrong_password(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (_client, _user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Create a new client to avoid using the same session
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let login_response =
            login_user_response(&client, &app, "john@doe.name".parse().unwrap(), "wrong").await;

        assert_eq!(login_response.status(), http::StatusCode::UNAUTHORIZED);
        let body: HashMap<String, String> = login_response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            "Unauthorized access: Invalid email address or password"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_login_with_unknown_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let login_response =
            login_user_response(&client, &app, "unknown@doe.name".parse().unwrap(), "").await;

        assert_eq!(login_response.status(), http::StatusCode::UNAUTHORIZED);
        let body: HashMap<String, String> = login_response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            "Unauthorized access: Invalid email address or password"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_logout_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (client, _user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let logout_response = logout_user_response(&client, &app.api_address).await;

        for cookie in logout_response.cookies() {
            assert_eq!(cookie.name(), "id");
            assert_eq!(cookie.value(), "");
            assert!(cookie.expires().unwrap() < SystemTime::now());
        }
        assert_eq!(logout_response.status(), http::StatusCode::OK);

        let response = get_current_user_response(&client, &app).await;

        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
        // Cookies are reset on unauthorized access in case of malformed cookies
        for cookie in response.cookies() {
            assert_eq!(cookie.name(), "id");
            assert_eq!(cookie.value(), "");
            assert!(cookie.expires().unwrap() < SystemTime::now());
        }
    }
}

mod email_verification {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_send_email_verification(
        settings: Settings,
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let (client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let first_email_validation_token = get_user_email_validation_token(&app, user.id)
            .await
            .unwrap();

        let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
        assert_eq!(emails_sent.len(), 1);

        let response = client
            .post(format!("{}users/me/email-verification", app.api_address))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::OK);

        let email_validation_token = get_user_email_validation_token(&app, user.id)
            .await
            .unwrap();

        assert!(first_email_validation_token != email_validation_token);

        let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
        assert_eq!(emails_sent.len(), 2);
        assert_eq!(emails_sent[1].0.id, user.id);
        assert_eq!(
            emails_sent[1].1,
            EmailTemplate::EmailVerification {
                first_name: None,
                email_verification_url: format!(
                    "{}users/{}/email-verification/{email_validation_token}",
                    settings.application.front_base_url, user.id
                )
                .parse()
                .unwrap()
            }
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_verify_email(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;
        let email_validation_token = get_user_email_validation_token(&app, user.id)
            .await
            .unwrap();

        let anonymous_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Email template contains frontend URL which is supposed to call this API endpoint
        let api_email_verification_url = format!(
            "{}users/{}/email-verification/{email_validation_token}",
            app.api_address, user.id
        );
        let response = anonymous_client
            .get(api_email_verification_url)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::OK);

        let user = get_current_user(&client, &app).await;

        assert!(user.email_validated_at.is_some());
        assert!(user.email_validation_sent_at.is_some());
    }

    #[rstest]
    #[tokio::test]
    async fn test_verify_email_unknown_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (_, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;
        let email_validation_token = get_user_email_validation_token(&app, user.id)
            .await
            .unwrap();

        let anonymous_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let user_id = UserId(Uuid::new_v4());
        // Email template contains frontend URL which is supposed to call this API endpoint
        let api_email_verification_url = format!(
            "{}users/{user_id}/email-verification/{email_validation_token}",
            app.api_address,
        );
        let response = anonymous_client
            .get(api_email_verification_url)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            format!("Invalid input data: Invalid email validation token for user {user_id}")
                .as_str()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_verify_email_invalid_token(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (_, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;
        let email_validation_token = EmailValidationToken(Uuid::new_v4());

        let anonymous_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Email template contains frontend URL which is supposed to call this API endpoint
        let api_email_verification_url = format!(
            "{}users/{}/email-verification/{email_validation_token}",
            app.api_address, user.id
        );
        let response = anonymous_client
            .get(api_email_verification_url)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            format!(
                "Invalid input data: Invalid email validation token for user {}",
                user.id
            )
            .as_str()
        );
    }
}

mod password_reset {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_send_password_reset_email(
        settings: Settings,
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;
        let email: EmailAddress = "john@doe.name".parse().unwrap();

        let (_client, user) = register_user(&app, email.clone(), "Very-harD-pasSword-5").await;

        let anonymous_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = anonymous_client
            .post(format!("{}users/password-reset", app.api_address))
            .json(&email)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::OK);

        let password_reset_token = get_password_reset_token(&app, user.id).await.unwrap();

        let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
        assert_eq!(emails_sent.len(), 2);
        assert_eq!(emails_sent[1].0.id, user.id);
        assert_eq!(
            emails_sent[1].1,
            EmailTemplate::PasswordReset {
                first_name: None,
                password_reset_url: format!(
                    "{}users/{}/password-reset/{password_reset_token}",
                    settings.application.front_base_url, user.id
                )
                .parse()
                .unwrap()
            }
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_reset_password(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;
        let email: EmailAddress = "john@doe.name".parse().unwrap();

        let (_client, user) = register_user(&app, email.clone(), "Very-harD-pasSword-5").await;

        let new_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = new_client
            .post(format!("{}users/password-reset", app.api_address))
            .json(&email)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);

        let password_reset_token = get_password_reset_token(&app, user.id).await.unwrap();
        // Email template contains frontend URL which is supposed to call this API endpoint
        let api_password_reset_url = format!(
            "{}users/{}/password-reset/{password_reset_token}",
            app.api_address, user.id
        );
        let response = new_client
            .post(api_password_reset_url)
            .json(&Password::from_str("New-very-harD-pasSword-5").unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);

        let password_reset_token = get_password_reset_token(&app, user.id).await;
        assert!(password_reset_token.is_none());

        let login_response =
            login_user_response(&new_client, &app, email.clone(), "New-very-harD-pasSword-5").await;
        assert_eq!(login_response.status(), http::StatusCode::OK);

        let user = get_current_user(&new_client, &app).await;
        let user_auth = get_user_auth(&app, user.id, UserAuthKind::Local).await;
        if let UserAuth::Local(local_user_auth) = user_auth {
            assert!(local_user_auth.password_reset_at.is_some());
            assert!(local_user_auth.password_reset_sent_at.is_some());
        } else {
            panic!("User should have local auth");
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_reset_password_unknown_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;
        let email: EmailAddress = "john@doe.name".parse().unwrap();

        let (_, user) = register_user(&app, email.clone(), "Very-harD-pasSword-5").await;

        let anonymous_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = anonymous_client
            .post(format!("{}users/password-reset", app.api_address))
            .json(&email)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);

        let password_reset_token = get_password_reset_token(&app, user.id).await.unwrap();
        let unknown_user_id = UserId(Uuid::new_v4());
        let api_password_reset_url = format!(
            "{}users/{unknown_user_id}/password-reset/{password_reset_token}",
            app.api_address
        );

        let response = anonymous_client
            .post(api_password_reset_url)
            .json(&Password::from_str("New-very-harD-pasSword-5").unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);

        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            format!("Invalid input data: Invalid password reset token for user {unknown_user_id}")
                .as_str()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_reset_password_invalid_token(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;
        let email: EmailAddress = "john@doe.name".parse().unwrap();

        let (_, user) = register_user(&app, email.clone(), "Very-harD-pasSword-5").await;

        let anonymous_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = anonymous_client
            .post(format!("{}users/password-reset", app.api_address))
            .json(&email)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);

        let invalid_password_reset_token = PasswordResetToken(Uuid::new_v4());
        let api_password_reset_url = format!(
            "{}users/{}/password-reset/{invalid_password_reset_token}",
            app.api_address, user.id
        );

        let response = anonymous_client
            .post(api_password_reset_url)
            .json(&Password::from_str("New-very-harD-pasSword-5").unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);

        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            format!(
                "Invalid input data: Invalid password reset token for user {}",
                user.id
            )
            .as_str()
        );
    }
}

mod create_authentication_token {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_create_authentication_token(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        let auth_token: AuthenticationToken = app
            .client
            .post(format!(
                "{}users/me/authentication-tokens",
                app.app.api_address
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(auth_token.user_id, app.user.id);
        assert!(!auth_token.is_session_token);
        assert!(!auth_token.is_revoked);
        assert!(!auth_token.is_expired());

        let auth_tokens = fetch_auth_tokens_for_user(&app.app, app.user.id).await;
        assert_eq!(auth_tokens.len(), 1);
        assert_eq!(auth_tokens[0].id, auth_token.id);
    }
}

mod patch_user {
    use super::*;
    use crate::helpers::tested_app_with_domain_blacklist;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_user_first_and_last_name(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let patch = UserPatch {
            first_name: Some("John".to_string()),
            last_name: Some("Doe".to_string()),
            email: None,
        };

        let response = patch_user_response(&client, &app, &patch).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let patched_user: User = response.json().await.unwrap();
        assert_eq!(patched_user.first_name, Some("John".to_string()));
        assert_eq!(patched_user.last_name, Some("Doe".to_string()));
        assert_eq!(patched_user.email, user.email);

        // Verify via GET
        let fetched_user = get_current_user(&client, &app).await;
        assert_eq!(fetched_user.first_name, Some("John".to_string()));
        assert_eq!(fetched_user.last_name, Some("Doe".to_string()));
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_user_email_resets_validation(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let (client, user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // First verify email
        let email_validation_token = get_user_email_validation_token(&app, user.id)
            .await
            .unwrap();

        let anonymous_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let api_email_verification_url = format!(
            "{}users/{}/email-verification/{email_validation_token}",
            app.api_address, user.id
        );
        anonymous_client
            .get(api_email_verification_url)
            .send()
            .await
            .unwrap();

        let verified_user = get_current_user(&client, &app).await;
        assert!(verified_user.email_validated_at.is_some());

        // Now change email
        let patch = UserPatch {
            first_name: None,
            last_name: None,
            email: Some("new@email.name".parse().unwrap()),
        };

        let response = patch_user_response(&client, &app, &patch).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let patched_user: User = response.json().await.unwrap();
        assert_eq!(patched_user.email, Some("new@email.name".parse().unwrap()));
        assert!(patched_user.email_validated_at.is_none());
        assert!(patched_user.email_validation_sent_at.is_some());
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_user_same_values_returns_not_modified(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let (client, _user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Patch with the same email
        let patch = UserPatch {
            first_name: None,
            last_name: None,
            email: Some("john@doe.name".parse().unwrap()),
        };

        let response = patch_user_response(&client, &app, &patch).await;
        assert_eq!(response.status(), http::StatusCode::NOT_MODIFIED);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_user_unauthenticated(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let anonymous_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let patch = UserPatch {
            first_name: Some("John".to_string()),
            last_name: None,
            email: None,
        };

        let response = patch_user_response(&anonymous_client, &app, &patch).await;
        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_user_duplicate_email(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        // Register first user
        let (client, _user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Register second user
        let _ = register_user(
            &app,
            "jane@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // First user tries to change email to second user's email
        let patch = UserPatch {
            first_name: None,
            last_name: None,
            email: Some("jane@doe.name".parse().unwrap()),
        };

        let response = patch_user_response(&client, &app, &patch).await;
        assert_eq!(response.status(), http::StatusCode::CONFLICT);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_user_empty_patch(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (client, _user) = register_user(
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let patch = UserPatch {
            first_name: None,
            last_name: None,
            email: None,
        };

        let response = patch_user_response(&client, &app, &patch).await;
        // Empty patch returns NotFound because update_user_profile returns early with result: None
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_user_blacklisted_email_domain(
        #[future] tested_app_with_domain_blacklist: TestedApp,
    ) {
        let app = tested_app_with_domain_blacklist.await;

        let (client, _user) = register_user(
            &app,
            "john@allowed.com".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let patch = UserPatch {
            first_name: None,
            last_name: None,
            email: Some("john@blocked.com".parse().unwrap()),
        };

        let response = patch_user_response(&client, &app, &patch).await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
        let body: HashMap<String, String> = response.json().await.unwrap();
        assert_eq!(
            body.get("message").unwrap(),
            "Forbidden access: Registration is not allowed from this domain"
        );
    }
}

/// The auth-state-changing user endpoints (login, register, email/password-reset,
/// passkey ceremonies) had no rate limit, allowing password brute force,
/// account-creation spam, email-bombing, and WebAuthn flood from a single IP.
/// The fix installs a shared per-IP governor limiter (30 req/min, mirroring
/// the OAuth2 limiter at `api/src/routes/oauth2.rs`) keyed on the real client
/// IP from `ConnectionInfo::realip_remote_addr()`.
///
/// One end-to-end test proves the wiring: a flood of `POST /users/me`
/// (login_user) from a single forwarded IP eventually returns `429 Too Many
/// Requests`, while a different forwarded IP still gets through. Per-endpoint
/// coverage is unnecessary because every affected handler shares the same
/// `check_ip_rate_limit` call.
mod auth_rate_limit {
    use super::*;
    use serde_json::json;

    #[rstest]
    #[tokio::test]
    async fn test_auth_endpoints_are_ip_rate_limited(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Bucket: 30 req/min (burst 30, refill 1 token / 2 s). Fire 35 from
        // a single forwarded IP **concurrently** so all of them hit the
        // limiter inside a window much smaller than the refill interval —
        // sequential dispatch under a contended runtime (full API suite
        // running ~num-cpus tests in parallel) can stretch past 10 s, during
        // which 5+ tokens refill and the burst is never exhausted.
        // `check_ip_rate_limit` is an atomic CAS, so concurrent dispatch is
        // safe and deterministic.
        let url = format!("{}users/me", app.api_address);
        let body = json!({
            "email": "nobody@example.com",
            "password": "wrong-password",
        });

        let requests = (0..35).map(|_| {
            client
                .post(&url)
                .header("X-Forwarded-For", "203.0.113.42")
                .json(&body)
                .send()
        });
        let statuses: Vec<reqwest::StatusCode> = futures::future::join_all(requests)
            .await
            .into_iter()
            .map(|r| r.expect("Failed to execute login request").status())
            .collect();

        assert!(
            statuses
                .iter()
                .contains(&reqwest::StatusCode::TOO_MANY_REQUESTS),
            "Expected POST /users/me to return 429 once the per-IP budget \
             was exhausted; observed statuses were {:?}",
            statuses
        );

        // A fresh forwarded IP must still be served — proving the limiter
        // is keyed per-IP and is not a global throttle. We assert only that
        // the response is NOT 429 (the actual status is 401 Unauthorized for
        // unknown credentials).
        let other_ip_status = client
            .post(&url)
            .header("X-Forwarded-For", "198.51.100.7")
            .json(&body)
            .send()
            .await
            .expect("Failed to execute login request from second IP")
            .status();

        assert_ne!(
            other_ip_status,
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "Expected a different X-Forwarded-For to bypass the exhausted \
             bucket of 203.0.113.42, but got 429"
        );
    }
}

/// The unauthenticated passkey start endpoints had two enumeration oracles:
///
///   * `POST /users/passkeys/registration/start` returned
///     `AlreadyExists { id: user_id.0 }` when the supplied username was
///     already registered, leaking the existing account's `UserId` (UUID)
///     to any unauthenticated caller in the `409 Conflict` body.
///
///   * `POST /users/passkeys/authentication/start` returned a `400 Bad
///     Request` with `ItemNotFound: No user found for username <X>` for
///     unknown usernames, while known usernames received a `200 OK` with
///     a `RequestChallengeResponse`, allowing username/email enumeration
///     by status-code probing.
///
/// The fix synthesizes shape-identical responses regardless of whether
/// the username exists: registration always returns a fresh ephemeral
/// `UserId`-bound challenge, and authentication mints a discoverable-
/// style challenge with an empty `allow_credentials` list for unknown
/// usernames. The HTTP status and body envelope are identical, and no
/// `user_id` or username text reaches the client on the "exists" or
/// "missing" paths.
mod passkey_start_non_enumerable {
    use super::*;
    use pretty_assertions::assert_eq;
    use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
    use webauthn_rs::prelude::{CreationChallengeResponse, RequestChallengeResponse};

    use crate::helpers::user::{
        finish_passkey_registration_response_unauthenticated,
        start_passkey_authentication_response, start_passkey_registration_response,
    };

    /// Drive the full unauthenticated passkey registration ceremony so a
    /// known passkey user exists in the database. Returns the username.
    async fn register_passkey_user(app: &TestedApp, username: &str) {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let start_response = start_passkey_registration_response(&client, app, username).await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let body = start_response.text().await.unwrap();
        let (creation_challenge, nonce) = crate::helpers::user::split_creation_challenge(&body);

        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration with software authenticator");

        let finish_response = finish_passkey_registration_response_unauthenticated(
            &client,
            app,
            &register_credential,
            &nonce,
        )
        .await;
        assert_eq!(finish_response.status(), reqwest::StatusCode::OK);
    }

    #[rstest]
    #[tokio::test]
    async fn test_start_passkey_registration_is_non_enumerable(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let registered_username = "alice_existing_passkey";
        register_passkey_user(&app, registered_username).await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Probe with a fresh username — the baseline success path.
        let fresh_response =
            start_passkey_registration_response(&client, &app, "bob_fresh_passkey").await;
        let fresh_status = fresh_response.status();
        assert_eq!(
            fresh_status,
            reqwest::StatusCode::OK,
            "fresh username must succeed"
        );
        let fresh_body = fresh_response.text().await.unwrap();
        let fresh_challenge: CreationChallengeResponse = serde_json::from_str(&fresh_body)
            .expect("fresh response must be a CreationChallengeResponse");

        // Probe with the same username we just registered. The fix MUST
        // return a CreationChallengeResponse with HTTP 200, not a 409
        // Conflict with a leaked UserId UUID.
        let exists_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let exists_response =
            start_passkey_registration_response(&exists_client, &app, registered_username).await;
        let exists_status = exists_response.status();

        assert_eq!(
            exists_status, fresh_status,
            "existing-username response status must match fresh-username status",
        );
        let exists_body = exists_response.text().await.unwrap();
        let exists_challenge: CreationChallengeResponse = serde_json::from_str(&exists_body)
            .expect(
                "existing-username response must also be a CreationChallengeResponse, not an \
                 AlreadyExists error envelope",
            );

        // The two responses must be structurally identical: same RP id,
        // same user.name, same user.displayName. Only the challenge bytes
        // and the synthetic user.id are allowed to differ (the user.id
        // for the registered case MUST be a fresh ephemeral UUID and
        // never the real account's UserId).
        assert_eq!(
            fresh_challenge.public_key.rp.id, exists_challenge.public_key.rp.id,
            "RP id must be identical"
        );
        assert_eq!(
            fresh_challenge.public_key.user.name, "bob_fresh_passkey",
            "fresh response user.name should echo the supplied username"
        );
        assert_eq!(
            exists_challenge.public_key.user.name, registered_username,
            "existing-username response should also echo the supplied username (parity)"
        );

        // No UUID may appear in the existing-username body except the
        // synthetic ephemeral user.id inside the CreationChallengeResponse.
        // In particular, no `AlreadyExists` message form ("The entity X
        // already exists") may be present.
        assert!(
            !exists_body.contains("already exists"),
            "existing-username response leaks an AlreadyExists error envelope: {exists_body}",
        );
        assert!(
            !exists_body.contains("AlreadyExists"),
            "existing-username response leaks an AlreadyExists variant name: {exists_body}",
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_start_passkey_authentication_is_non_enumerable(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let registered_username = "carol_registered_passkey";
        register_passkey_user(&app, registered_username).await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Known username — baseline success path.
        let known_response =
            start_passkey_authentication_response(&client, &app, registered_username).await;
        let known_status = known_response.status();
        assert_eq!(
            known_status,
            reqwest::StatusCode::OK,
            "known username must succeed with 200 OK"
        );
        let known_body = known_response.text().await.unwrap();
        let known_challenge: RequestChallengeResponse = serde_json::from_str(&known_body)
            .expect("known-username response must be a RequestChallengeResponse");

        // Unknown username — pre-fix this returned 400 ItemNotFound with
        // the username echoed. The fix MUST produce a 200 OK
        // RequestChallengeResponse instead.
        let unknown_username = "ghost_not_registered_passkey";
        let unknown_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let unknown_response =
            start_passkey_authentication_response(&unknown_client, &app, unknown_username).await;
        let unknown_status = unknown_response.status();

        assert_eq!(
            unknown_status, known_status,
            "unknown-username status must match known-username status",
        );
        let unknown_body = unknown_response.text().await.unwrap();
        let unknown_challenge: RequestChallengeResponse = serde_json::from_str(&unknown_body)
            .expect(
                "unknown-username response must be a RequestChallengeResponse, not an \
                 ItemNotFound error envelope",
            );

        // The unknown-username body MUST NOT echo the supplied username,
        // and MUST NOT carry an ItemNotFound message form.
        assert!(
            !unknown_body.contains(unknown_username),
            "unknown-username response leaks the supplied username: {unknown_body}",
        );
        assert!(
            !unknown_body.contains("Item not found"),
            "unknown-username response leaks an ItemNotFound envelope: {unknown_body}",
        );
        assert!(
            !unknown_body.contains("ItemNotFound"),
            "unknown-username response leaks an ItemNotFound variant name: {unknown_body}",
        );

        // RP id parity — the field that matters most for shape parity.
        assert_eq!(
            known_challenge.public_key.rp_id, unknown_challenge.public_key.rp_id,
            "RP id must be identical across known/unknown branches",
        );
    }
}

/// `POST /users/passkeys/registration/finish` previously surfaced the
/// `user_auth_username_key` unique-constraint violation as a raw
/// `DatabaseError`. The HTTP body was:
///
/// ```json
/// {"message":"Database error: Failed to create user auth for user <UUID>: \
///   error returned from database: duplicate key value violates unique \
///   constraint \"user_auth_username_key\""}
/// ```
///
/// That body leaked the Postgres constraint name (a direct username-
/// collision signal), the raw sqlx error text, and the ephemeral
/// `UserId` UUID minted during the start ceremony. The fix maps the
/// `23505` on `user_auth_username_key` in `Repository::create_user_auth`
/// to a `UniversalInboxError::Conflict` with a generic user-facing
/// message, producing a `409 Conflict` body that carries no internal
/// implementation details.
mod passkey_finish_non_leaking {
    use super::*;
    use pretty_assertions::assert_eq;
    use regex::Regex;
    use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
    use webauthn_rs::prelude::CreationChallengeResponse;

    use crate::helpers::user::{
        finish_passkey_registration_response_unauthenticated, split_creation_challenge,
        start_passkey_registration_response,
    };

    #[rstest]
    #[tokio::test]
    async fn test_finish_passkey_registration_with_taken_username_does_not_leak(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        // 1. Register a passkey user so the username is taken.
        let registered_username = "alice_taken_username";
        register_passkey_user(&app, registered_username).await;

        // 2. Drive a second start ceremony for the same username with a
        //    fresh client (no shared session/cookie state) and a fresh
        //    SoftPasskey. The start endpoint is already non-enumerable
        //    (covered by `passkey_start_non_enumerable`); it returns 200
        //    with a CreationChallengeResponse bound to an ephemeral
        //    UserId. We need that challenge to drive the authenticator.
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let start_response =
            start_passkey_registration_response(&client, &app, registered_username).await;
        assert_eq!(
            start_response.status(),
            reqwest::StatusCode::OK,
            "start endpoint must remain non-enumerable for taken username"
        );
        let (creation_challenge, nonce) =
            split_creation_challenge(&start_response.text().await.unwrap());

        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration with software authenticator");

        // 3. Call finish; the unique-constraint violation must surface as
        //    a clean 409 Conflict, not a raw DatabaseError.
        let finish_response = finish_passkey_registration_response_unauthenticated(
            &client,
            &app,
            &register_credential,
            &nonce,
        )
        .await;
        let finish_status = finish_response.status();
        let finish_body = finish_response.text().await.unwrap();

        assert_eq!(
            finish_status,
            reqwest::StatusCode::CONFLICT,
            "duplicate-username finish must be 409 Conflict, body was: {finish_body}",
        );

        let parsed: serde_json::Value =
            serde_json::from_str(&finish_body).expect("finish response body must parse as JSON");
        let message = parsed
            .get("message")
            .and_then(|v| v.as_str())
            .expect("finish response body must have a string `message` field");
        assert_eq!(
            message, "This username is already taken. Please choose a different one.",
            "finish response message must match the generic user-facing copy verbatim",
        );

        // 4. The body must not leak any internal implementation details.
        let forbidden_substrings = [
            "user_auth_username_key",
            "duplicate key",
            "Database error",
            "DatabaseError",
            "unique constraint",
            "AlreadyExists",
            "sqlx",
            registered_username,
        ];
        for needle in &forbidden_substrings {
            assert!(
                !finish_body.contains(needle),
                "finish response leaks {needle:?}: {finish_body}",
            );
        }

        // No UUID may appear in the response body. The ephemeral UserId
        // minted at start time is the most sensitive value to suppress
        // here, but a stricter check that no UUID-shaped substring appears
        // also catches the unlikely case that some other id leaks.
        let uuid_regex = Regex::new(
            r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        )
        .unwrap();
        assert!(
            !uuid_regex.is_match(&finish_body),
            "finish response leaks a UUID: {finish_body}",
        );
    }

    /// Drive the full unauthenticated passkey registration ceremony so a
    /// known passkey user exists in the database. Duplicated from
    /// `passkey_start_non_enumerable` to keep the modules independent.
    async fn register_passkey_user(app: &TestedApp, username: &str) {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let start_response = start_passkey_registration_response(&client, app, username).await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let (creation_challenge, nonce) =
            split_creation_challenge(&start_response.text().await.unwrap());

        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration with software authenticator");

        let finish_response = finish_passkey_registration_response_unauthenticated(
            &client,
            app,
            &register_credential,
            &nonce,
        )
        .await;
        assert_eq!(finish_response.status(), reqwest::StatusCode::OK);
    }
}

/// Regression tests for universal-inbox-bkj.32: per-ceremony nonce
/// binding, Origin/Referer check, and cross-flow Redis-prefix isolation
/// for the four passkey start/finish handlers.
///
/// The pre-fix passkey ceremony handlers relied solely on the cookie
/// session + a Redis state blob keyed by `user_id`. There was no
/// per-flow secret the frontend had to echo back, no Origin/Referer
/// check, and a tampered or cross-flow session could in principle pair
/// with state from another flow. The handlers now:
///
///   - generate a fresh per-ceremony nonce at start, return it to the
///     client, and require the client to echo it on finish;
///   - reject any request whose `Origin` (or `Referer`) does not match
///     the configured `application.front_base_url`;
///   - keep the ADD-passkey-to-existing-user flow and the
///     INITIAL-passkey-registration flow in disjoint session/Redis key
///     namespaces so they can never be cross-consumed.
mod passkey_ceremony_csrf_hardening {
    use super::*;
    use pretty_assertions::assert_eq;
    use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
    use webauthn_rs::prelude::CreationChallengeResponse;

    use crate::helpers::user::{
        finish_add_passkey_registration_response,
        finish_passkey_registration_response_unauthenticated, front_origin_header, register_user,
        split_creation_challenge, start_add_passkey_registration_response,
        start_passkey_registration_response,
    };

    /// Verify that the start response includes a `nonce` and that a
    /// finish call with a *mismatched* nonce is rejected, while the
    /// matching nonce succeeds.
    #[rstest]
    #[tokio::test]
    async fn test_finish_passkey_registration_requires_matching_nonce(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Start the unauthenticated passkey registration flow.
        let start_response =
            start_passkey_registration_response(&client, &app, "nonce_match_alice").await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let start_body = start_response.text().await.unwrap();
        let (creation_challenge, server_nonce) = split_creation_challenge(&start_body);

        // The start response MUST include a non-empty nonce.
        assert!(
            !server_nonce.is_empty(),
            "start response must include a non-empty nonce"
        );

        // Complete the WebAuthn challenge so we have a valid credential
        // payload to send to finish.
        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration");

        // First attempt: a deliberately mismatched nonce must be
        // rejected (401 per the existing Unauthorized convention used
        // for session-bound CSRF rejection).
        let bogus_nonce = "AAAAAAAAAAAAAAAAAAAAAA";
        assert_ne!(
            bogus_nonce, server_nonce,
            "test pre-condition: the bogus nonce must differ from the server-issued one"
        );
        let bad_response = finish_passkey_registration_response_unauthenticated(
            &client,
            &app,
            &register_credential,
            bogus_nonce,
        )
        .await;
        assert_eq!(
            bad_response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "finish with mismatched nonce must be rejected"
        );

        // Second attempt: even with the wrong nonce above, the start
        // state was already consumed (the finish handler `get_del`s the
        // Redis blob on first read), so we cannot re-use the same
        // ceremony with the correct nonce — drive a fresh ceremony and
        // verify the happy path with the right nonce.
        let fresh_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let start2 =
            start_passkey_registration_response(&fresh_client, &app, "nonce_match_alice_v2").await;
        assert_eq!(start2.status(), reqwest::StatusCode::OK);
        let (challenge2, good_nonce) = split_creation_challenge(&start2.text().await.unwrap());
        let mut auth2 = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let cred2 = auth2
            .do_registration(app.front_base_url.clone(), challenge2)
            .expect("Failed to complete passkey registration");
        let good_response = finish_passkey_registration_response_unauthenticated(
            &fresh_client,
            &app,
            &cred2,
            &good_nonce,
        )
        .await;
        assert_eq!(
            good_response.status(),
            reqwest::StatusCode::OK,
            "finish with the server-issued nonce must succeed"
        );
    }

    /// A finish call carrying an `Origin` header pointing at a
    /// foreign origin must be rejected; the matching origin still
    /// succeeds.
    #[rstest]
    #[tokio::test]
    async fn test_finish_passkey_registration_rejects_foreign_origin(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let start_response =
            start_passkey_registration_response(&client, &app, "origin_check_bob").await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let (challenge, nonce) = split_creation_challenge(&start_response.text().await.unwrap());

        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let credential = authenticator
            .do_registration(app.front_base_url.clone(), challenge)
            .expect("Failed to complete passkey registration");

        // Build a finish call by hand so we can swap the Origin header.
        let body = crate::helpers::user::finish_body_with_nonce(&credential, &nonce);
        let evil_response = client
            .post(format!(
                "{}users/passkeys/registration/finish",
                app.api_address
            ))
            .header(reqwest::header::ORIGIN, "https://evil.example")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            evil_response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "finish with foreign Origin must be rejected with 400"
        );

        // The state is still in place (the foreign-origin call was
        // rejected before any state consumption). Retry from the same
        // client with the correct Origin header and the same nonce —
        // it must succeed.
        let ok_response = client
            .post(format!(
                "{}users/passkeys/registration/finish",
                app.api_address
            ))
            .header(reqwest::header::ORIGIN, front_origin_header(&app))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            ok_response.status(),
            reqwest::StatusCode::OK,
            "finish with the configured front-end Origin must succeed"
        );
    }

    /// Cross-flow / cross-prefix isolation: the ADD-passkey-to-existing
    /// -user flow (authenticated, `/users/me/auth-methods/passkey/...`)
    /// uses session key `add-passkey-registration-state` and Redis
    /// prefix `add-passkey-registration-state::{user_id}`. The
    /// INITIAL-passkey-registration flow (unauthenticated,
    /// `/users/passkeys/...`) uses key `passkey-registration-state`.
    /// Starting an ADD flow MUST NOT leave state that the INITIAL
    /// finish handler can consume, even when both flows would (pre-fix)
    /// have keyed Redis by the same `user_id`.
    ///
    /// The session-key separation alone is sufficient to demonstrate
    /// the isolation: the INITIAL finish handler reads
    /// `passkey-registration-state` from the session and finds nothing
    /// when only the ADD flow's `add-passkey-registration-state` was
    /// set.
    #[rstest]
    #[tokio::test]
    async fn test_add_passkey_state_cannot_complete_initial_passkey_finish(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        // Register a local user and authenticate the client.
        let (client, _user) = register_user(
            &app,
            "carol@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Start the ADD-passkey-to-existing-user flow — this sets up
        // the ADD-flow session key and Redis blob.
        let start_response =
            start_add_passkey_registration_response(&client, &app, "carol_passkey").await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let (creation_challenge, add_nonce): (CreationChallengeResponse, String) =
            split_creation_challenge(&start_response.text().await.unwrap());

        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(app.front_base_url.clone(), creation_challenge)
            .expect("Failed to complete passkey registration");

        // Crossover attempt: try to consume the ADD-flow state via the
        // INITIAL passkey finish endpoint. The INITIAL endpoint reads
        // `passkey-registration-state` from the session, which was
        // never written, so the call must be rejected. (Before the
        // session-key split + nonce binding, an attacker who could
        // confuse the routing by user_id might have driven state from
        // one flow into the other; the current handlers cannot.)
        let cross_flow_body =
            crate::helpers::user::finish_body_with_nonce(&register_credential, &add_nonce);
        let cross_response = client
            .post(format!(
                "{}users/passkeys/registration/finish",
                app.api_address
            ))
            .header(reqwest::header::ORIGIN, front_origin_header(&app))
            .json(&cross_flow_body)
            .send()
            .await
            .unwrap();
        assert!(
            !cross_response.status().is_success(),
            "ADD-flow state must not be consumable via the INITIAL finish endpoint, but \
             got {}",
            cross_response.status()
        );

        // Sanity check: the proper ADD finish endpoint, with the same
        // credential and the correct nonce, must succeed — proving the
        // ADD state itself is intact and only the cross-flow attempt
        // above was blocked by the session-key namespacing.
        let proper_response = finish_add_passkey_registration_response(
            &client,
            &app,
            &register_credential,
            &add_nonce,
        )
        .await;
        assert_eq!(
            proper_response.status(),
            reqwest::StatusCode::OK,
            "ADD finish with the correct endpoint + nonce must succeed"
        );
    }
}
