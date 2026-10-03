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
                               // The 'sha256-…' source is derived at startup from the inline
                               // <script> in tests/api/statics/index.html (body `console.log("test");`),
                               // standing in for Trunk's injected WASM bootstrap.
                               "default-src 'self'; script-src 'self' 'wasm-unsafe-eval' https://client.crisp.chat https://cdn.headwayapp.co 'sha256-uAESwGgY2G0W8BhcAjQ5tDZK88YZcbjq65DW8JTcims='; style-src 'self' 'unsafe-inline' https://client.crisp.chat; object-src 'none'; connect-src 'self' {} https://client.crisp.chat wss://client.relay.crisp.chat; img-src * 'self' data:; font-src 'self' https://client.crisp.chat; worker-src 'none'; frame-src 'self' https://headway-widget.net; frame-ancestors 'self'; base-uri 'self'; form-action 'self'",
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
        // HSTS must be set on every response to force HTTPS-only connections.
        assert_eq!(
            response.headers().get("strict-transport-security"),
            Some(&HeaderValue::from_static(
                "max-age=63072000; includeSubDomains; preload"
            ))
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
        // HSTS is emitted on every response, including JSON, to force
        // HTTPS-only connections for the host.
        assert_eq!(
            response.headers().get("strict-transport-security"),
            Some(&HeaderValue::from_static(
                "max-age=63072000; includeSubDomains; preload"
            ))
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

mod invalid_static_paths {
    use super::*;

    use pretty_assertions::assert_eq;

    #[rstest]
    #[case::dotfile("/.env")]
    #[case::nested_dotfile("/.git/config")]
    #[case::unknown_well_known("/.well-known/security.txt")]
    #[case::wildcard("/*")]
    #[case::nested_wildcard("/assets/*.js")]
    #[case::trailing_colon("/c:")]
    #[case::nested_trailing_colon("/foo:/bar")]
    #[case::trailing_lower_than("/foo%3C")]
    #[case::trailing_greater_than("/foo%3E")]
    // Real scanner probes seen in production traces
    #[case::encoded_slash("/admin%2F.env")]
    #[case::encoded_lowercase_slash_traversal("/static/%2e%2e%2f%2e%2e%2f.env")]
    #[case::encoded_dot_after_wildcard("/*%2eenv%2esave")]
    #[case::encoded_dotfile("/home/user/%2eaws/credentials")]
    #[case::url_as_path("/https://app.universal-inbox.com/")]
    #[tokio::test]
    async fn test_invalid_static_path_returns_not_found(
        #[future] tested_app: TestedApp,
        #[case] path: &str,
    ) {
        let app = tested_app.await;

        let response = reqwest::Client::new()
            .get(format!("{}{path}", app.app_address))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 404);
    }

    #[rstest]
    #[case::inner_dot("/assets/app.min.js")]
    #[case::inner_colon("/notifications/a:b")]
    #[case::inner_wildcard("/foo*bar")]
    #[tokio::test]
    async fn test_valid_static_path_is_served(#[future] tested_app: TestedApp, #[case] path: &str) {
        let app = tested_app.await;

        let response = reqwest::Client::new()
            .get(format!("{}{path}", app.app_address))
            .send()
            .await
            .unwrap();

        // Unknown SPA routes fall back to index.html
        assert_eq!(response.status(), 200);
    }

    #[rstest]
    #[tokio::test]
    async fn test_registered_well_known_route_still_served(#[future] tested_app: TestedApp) {
        let app = tested_app.await;

        let response = reqwest::Client::new()
            .get(format!(
                "{}/.well-known/oauth-authorization-server",
                app.app_address
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
    }
}

mod static_files {
    use super::*;

    use pretty_assertions::assert_eq;

    const HASHED_WASM_PATH: &str = "/app-0123456789abcdef_bg.wasm";
    const IMMUTABLE: &str = "public, max-age=31536000, immutable";

    async fn get(app: &TestedApp, path: &str, accept_encoding: &str) -> reqwest::Response {
        // Keep the raw (compressed) body and headers: no transparent decompression
        reqwest::Client::builder()
            .no_gzip()
            .no_brotli()
            .no_zstd()
            .no_deflate()
            .build()
            .unwrap()
            .get(format!("{}{path}", app.app_address))
            .header("accept-encoding", accept_encoding)
            .send()
            .await
            .unwrap()
    }

    fn header<'a>(response: &'a reqwest::Response, name: &str) -> Option<&'a str> {
        response
            .headers()
            .get(name)
            .map(|value| value.to_str().unwrap())
    }

    #[rstest]
    #[case::brotli(
        "br, gzip",
        Some("br"),
        "tests/api/statics/app-0123456789abcdef_bg.wasm.br"
    )]
    #[case::gzip(
        "gzip",
        Some("gzip"),
        "tests/api/statics/app-0123456789abcdef_bg.wasm.gz"
    )]
    #[case::identity("identity", None, "tests/api/statics/app-0123456789abcdef_bg.wasm")]
    #[tokio::test]
    async fn test_hashed_asset_is_served_precompressed_and_immutable(
        #[future] tested_app: TestedApp,
        #[case] accept_encoding: &str,
        #[case] expected_content_encoding: Option<&str>,
        #[case] expected_body_file: &str,
    ) {
        let app = tested_app.await;

        let response = get(&app, HASHED_WASM_PATH, accept_encoding).await;

        assert_eq!(response.status(), 200);
        assert_eq!(
            header(&response, "content-encoding"),
            expected_content_encoding
        );
        assert_eq!(header(&response, "cache-control"), Some(IMMUTABLE));
        assert_eq!(header(&response, "content-type"), Some("application/wasm"));
        let expected_body = std::fs::read(expected_body_file).unwrap();
        assert_eq!(response.bytes().await.unwrap().to_vec(), expected_body);
    }

    #[rstest]
    #[case::root("/")]
    #[case::spa_route("/notifications")]
    #[case::spa_route_looking_hashed("/notifications/app-0123456789abcdef")]
    #[case::unhashed_asset("/style.css")]
    #[tokio::test]
    async fn test_unhashed_content_is_revalidated(
        #[future] tested_app: TestedApp,
        #[case] path: &str,
    ) {
        let app = tested_app.await;

        let response = get(&app, path, "identity").await;

        assert_eq!(response.status(), 200);
        assert_eq!(header(&response, "cache-control"), Some("no-cache"));
    }
}
