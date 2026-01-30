use http::HeaderValue;
use rstest::*;

use universal_inbox_api::configuration::Settings;

use crate::helpers::{settings, tested_app, TestedApp};

mod content_security_policy {
    use super::*;

    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_csp_header_on_html_page(#[future] tested_app: TestedApp, settings: Settings) {
        let app = tested_app.await;
        let mut nango_ws_base_url = settings.oauth2.nango_base_url.clone();
        nango_ws_base_url.set_scheme("ws").unwrap();

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
                               "default-src 'self'; script-src 'self' 'wasm-unsafe-eval' 'unsafe-inline' 'unsafe-eval' https://cdn.headwayapp.co; style-src 'self' 'unsafe-inline'; object-src 'none'; connect-src 'self' {} {} {}; img-src * 'self' data:; worker-src 'none'; frame-src 'self' https://headway-widget.net",
                               nango_ws_base_url, settings.oauth2.nango_base_url, app.oidc_issuer_mock_server.as_ref().unwrap().base_url()
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
        assert_eq!(
            response.headers().get("content-type"),
            Some(&HeaderValue::from_static("application/json"))
        );
        assert!(response.headers().get("content-security-policy").is_none());
    }
}
