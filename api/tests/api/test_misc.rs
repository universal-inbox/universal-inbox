use http::HeaderValue;
use rstest::*;

use crate::helpers::{TestedApp, tested_app};

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
                               "default-src 'self'; script-src 'self' 'wasm-unsafe-eval' 'unsafe-inline' 'unsafe-eval' https://client.crisp.chat https://cdn.headwayapp.co; style-src 'self' 'unsafe-inline' https://client.crisp.chat; object-src 'none'; connect-src 'self' {} https://client.crisp.chat wss://client.relay.crisp.chat; img-src * 'self' data:; font-src 'self' https://client.crisp.chat; worker-src 'none'; frame-src 'self' https://headway-widget.net; frame-ancestors 'self'",
                               app.oidc_issuer_mock_server.as_ref().unwrap().uri()
                           )
                       ).unwrap()
                   )
        );
        // X-Frame-Options: DENY must be set on every response (clickjacking
        // defense-in-depth alongside CSP frame-ancestors).
        assert_eq!(
            response.headers().get("x-frame-options"),
            Some(&HeaderValue::from_static("DENY"))
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
        // CSP is HTML-specific (the directives only meaningfully constrain a
        // browsing context), so we keep emitting it only on text/html
        // responses. The clickjacking-relevant header X-Frame-Options is
        // emitted on every response — see assertion below.
        assert!(response.headers().get("content-security-policy").is_none());
        // X-Frame-Options: DENY is emitted on every response, including JSON,
        // because a misconfigured CDN or reverse proxy could otherwise serve
        // a JSON-as-HTML attack surface.
        assert_eq!(
            response.headers().get("x-frame-options"),
            Some(&HeaderValue::from_static("DENY"))
        );
    }
}

mod cors {
    use super::*;

    use pretty_assertions::assert_ne;

    /// Cross-origin requests from a *non-front-base-url* origin to a regular
    /// cookie-authed API endpoint must NOT receive a credentialed CORS
    /// response. Previously the layer-wide `supports_credentials()` combined
    /// with the wildcard `allowed_origin_fn` for `/api/oauth2`, `/api/mcp`
    /// and `/.well-known/oauth-` paths echoed back the attacker origin with
    /// `Access-Control-Allow-Credentials: true`.
    #[rstest]
    #[tokio::test]
    async fn test_cors_rejects_arbitrary_origin_on_cookie_authed_endpoint(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;

        let response = reqwest::Client::new()
            .request(
                reqwest::Method::OPTIONS,
                format!("{}notifications", app.api_address),
            )
            .header("Origin", "https://evil.example")
            .header("Access-Control-Request-Method", "GET")
            .header("Access-Control-Request-Headers", "authorization")
            .send()
            .await
            .unwrap();

        // The CORS preflight must NOT echo the attacker origin.
        assert_ne!(
            response.headers().get("access-control-allow-origin"),
            Some(&HeaderValue::from_static("https://evil.example"))
        );
        // And in no case may we end up with `*` + `true` credentials combo.
        let allow_origin = response.headers().get("access-control-allow-origin");
        let allow_credentials = response.headers().get("access-control-allow-credentials");
        if allow_origin == Some(&HeaderValue::from_static("*")) {
            assert_ne!(
                allow_credentials,
                Some(&HeaderValue::from_static("true")),
                "wildcard origin combined with credentials would be a CORS misconfiguration"
            );
        }
    }

    /// Cross-origin requests from a *non-front-base-url* origin to the MCP
    /// scope are intentionally bearer-only; the CORS layer must therefore not
    /// emit `Access-Control-Allow-Credentials: true` for them, even when the
    /// origin is reflected back.
    #[rstest]
    #[tokio::test]
    async fn test_cors_mcp_endpoint_does_not_credential_arbitrary_origin(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;

        let response = reqwest::Client::new()
            .request(reqwest::Method::OPTIONS, format!("{}mcp/", app.api_address))
            .header("Origin", "https://evil.example")
            .header("Access-Control-Request-Method", "POST")
            .header("Access-Control-Request-Headers", "authorization")
            .send()
            .await
            .unwrap();

        // If the preflight is honored at all, the response must not carry
        // credentialed CORS for an unconfigured origin.
        if response
            .headers()
            .get("access-control-allow-origin")
            .is_some()
        {
            assert_ne!(
                response.headers().get("access-control-allow-credentials"),
                Some(&HeaderValue::from_static("true")),
                "MCP scope is bearer-only; credentialed CORS for arbitrary \
                 origins would re-introduce the universal-inbox-bkj.12 \
                 misconfiguration"
            );
        }
    }
}
