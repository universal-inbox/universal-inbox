use std::{collections::HashMap, str::FromStr};

use email_address::EmailAddress;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    auth::auth_token::AuthenticationToken,
    user::{EmailValidationToken, Password, PasswordResetToken, User, UserId},
};

use universal_inbox_api::{
    configuration::Settings, mailer::EmailTemplate, universal_inbox::user::model::UserAuth,
};

use crate::helpers::{
    auth::{authenticated_app, fetch_auth_tokens_for_user, get_user_auth, AuthenticatedApp},
    settings, tested_app_with_local_auth,
    user::{
        get_current_user, get_current_user_response, get_password_reset_token,
        get_user_email_validation_token, login_user_response, logout_user_response, register_user,
        register_user_response,
    },
    TestedApp,
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
            core::{CoreHmacKey, CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm},
            Audience, EmptyAdditionalClaims, EndUserEmail, IssuerUrl, StandardClaims,
            SubjectIdentifier,
        };

        let app = tested_app_with_domain_blacklist.await;

        // Set up OIDC mocks with a blacklisted domain email
        app.oidc_issuer_mock_server.as_ref().unwrap().reset().await;
        mock_oidc_openid_configuration(&app);
        mock_oidc_keys(&app);
        mock_oidc_introspection(&app, "1234", true);
        mock_oidc_user_info(&app, "1234", "John", "Doe", "user@blocked.com");

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Create an ID token
        let signing_key = CoreHmacKey::new("secret".as_bytes());
        let oidc_issuer_mock_server_url = app
            .oidc_issuer_mock_server
            .as_ref()
            .map(|s| s.base_url())
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
        let user_auth = get_user_auth(&app, user.id).await;
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
