use rstest::*;

use crate::helpers::{
    auth::{mock_oidc_introspection, mock_oidc_keys, mock_oidc_openid_configuration},
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
        mock_oidc_introspection(&app, true);

        let response = reqwest::Client::new()
            .get(&format!("{}/auth/session", app.app_address))
            .bearer_auth("fake_token")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
    }

    #[rstest]
    #[tokio::test]
    async fn test_authenticate_session_creation_wrong_access_token(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;

        mock_oidc_openid_configuration(&app);
        mock_oidc_keys(&app);
        mock_oidc_introspection(&app, false);

        let response = reqwest::Client::new()
            .get(&format!("{}/auth/session", app.app_address))
            .bearer_auth("fake_token")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 401);
    }
}
