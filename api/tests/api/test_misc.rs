use http::HeaderValue;
use rstest::*;

use crate::helpers::{tested_app, TestedApp};

mod content_security_policy {
    use super::*;

    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_csp_header_on_html_page(#[future] tested_app: TestedApp) {
        let app = tested_app.await;

        let response = reqwest::Client::new()
            .get(&app.app_address)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("content-type"),
            Some(&HeaderValue::from_static("text/html; charset=utf-8"))
        );
        assert_eq!(response.headers().get("content-security-policy"),
                   Some(
                       &HeaderValue::from_str(
                           &format!(
                               "default-src 'self'; script-src 'self' 'wasm-unsafe-eval' 'unsafe-inline' https://cdn.headwayapp.co; style-src 'self' 'unsafe-inline'; object-src 'none'; connect-src 'self' ws://localhost:3003/ http://localhost:3003/ {}; img-src 'self' https://secure.gravatar.com https://avatars.githubusercontent.com https://private-user-images.githubusercontent.com https://public.linear.app https://avatars.slack-edge.com data:; worker-src 'none'; frame-src 'self' https://headway-widget.net",
                               app.oidc_issuer_mock_server.as_ref().unwrap().base_url()
                           )
                       ).unwrap()
                   )
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_csp_header_on_other_url(#[future] tested_app: TestedApp) {
        let app = tested_app.await;

        let response = reqwest::Client::new()
            .get(format!("{}/ping", app.app_address))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        assert!(response.headers().get("content-type").is_none());
        assert!(response.headers().get("content-security-policy").is_none());
    }
}
