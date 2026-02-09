use std::collections::HashMap;

use rstest::*;

use universal_inbox::user::{UserAuthKind, UserAuthMethod, UserAuthMethodDisplayInfo};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app, get_all_user_auths},
    tested_app_with_domain_blacklist, tested_app_with_local_auth,
    user::{
        add_local_auth_response, list_auth_methods, list_auth_methods_response, register_user,
        remove_auth_method_response,
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
