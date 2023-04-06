use rstest::*;

use universal_inbox::user::User;

use crate::helpers::{
    auth::{
        mock_oidc_introspection, mock_oidc_keys, mock_oidc_openid_configuration,
        mock_oidc_user_info,
    },
    tested_app, TestedApp,
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

        assert_eq!(user.first_name, "John");
        assert_eq!(user.last_name, "Doe");
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
            .get(&format!("{}/auth/session", app.app_address))
            .bearer_auth("fake_token")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 401);
    }
}
