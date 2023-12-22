use rstest::*;

use crate::helpers::{
    tested_app_with_local_auth,
    user::{
        get_current_user, get_current_user_response, login_user_response, logout_user_response,
        register_user, register_user_response,
    },
    TestedApp,
};

mod authenticate_session {
    use std::collections::HashMap;

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_register_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (_client, user) = register_user(
            &app,
            "John",
            "Doe",
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        assert_eq!(user.first_name, "John");
        assert_eq!(user.last_name, "Doe");
        assert_eq!(user.email, "john@doe.name".parse().unwrap());
    }

    #[rstest]
    #[tokio::test]
    async fn test_register_existing_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (client, _user) = register_user(
            &app,
            "John",
            "Doe",
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let response = register_user_response(
            &client,
            &app,
            "John",
            "Doe",
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

    #[rstest]
    #[tokio::test]
    async fn test_login_user(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (_client, _user) = register_user(
            &app,
            "John",
            "Doe",
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Create a new client to avoid using the same session
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let response = get_current_user_response(&client, &app).await;

        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);

        let login_response = login_user_response(
            &client,
            &app,
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        assert_eq!(login_response.status(), http::StatusCode::OK);

        let user = get_current_user(&client, &app).await;

        assert_eq!(user.first_name, "John");
        assert_eq!(user.last_name, "Doe");
        assert_eq!(user.email, "john@doe.name".parse().unwrap());
    }

    #[rstest]
    #[tokio::test]
    async fn test_login_with_wrong_password(#[future] tested_app_with_local_auth: TestedApp) {
        let app = tested_app_with_local_auth.await;

        let (_client, _user) = register_user(
            &app,
            "John",
            "Doe",
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
            "John",
            "Doe",
            "john@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        let logout_response = logout_user_response(&client, &app.api_address).await;

        assert_eq!(logout_response.status(), http::StatusCode::OK);

        let response = get_current_user_response(&client, &app).await;

        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
    }
}
