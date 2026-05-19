//! Origin / Referer enforcement for cookie-credentialed state-changing
//! endpoints.
//!
//! Cookie-bearing endpoints that bind state into the session (passkey start
//! and finish ceremonies, add-passkey auth-method flows) must not be callable
//! from arbitrary origins. CORS already restricts pre-flighted browser calls
//! to `application.front_base_url`, but:
//!
//! - simple `application/json` POSTs are CORS-relaxed and can be reached
//!   cross-site without a preflight (the response is unreadable but the
//!   side effect — session mutation — still lands);
//! - direct non-browser callers ignore CORS entirely.
//!
//! Verifying that the request's `Origin` (or fallback `Referer`) matches the
//! configured front-end base URL — scheme + host + port — closes both holes
//! at the handler boundary. We refuse with `400 Bad Request` because the
//! request is malformed from the API's perspective, not unauthenticated.
//!
//! This helper is deliberately **not** applied to OAuth2 / MCP / well-known
//! endpoints: those must remain reachable from third-party origins per the
//! protocol.

use actix_web::{HttpRequest, HttpResponse, http::header};
use url::Url;

/// Verify that the incoming request's `Origin` (or `Referer` as fallback)
/// matches the configured front-end base URL on scheme + host + port.
///
/// Returns `Ok(())` if the origin matches, or `Err(HttpResponse)` with a
/// 400 envelope ready to be returned. Missing both headers, an
/// unparseable header, or a mismatch all yield `Err`.
pub fn check_request_origin(req: &HttpRequest, expected: &Url) -> Result<(), HttpResponse> {
    let raw = req
        .headers()
        .get(header::ORIGIN)
        .or_else(|| req.headers().get(header::REFERER))
        .and_then(|v| v.to_str().ok());

    let Some(raw) = raw else {
        return Err(reject("Missing Origin/Referer header"));
    };

    let actual = match Url::parse(raw) {
        Ok(url) => url,
        Err(_) => return Err(reject("Unparseable Origin/Referer header")),
    };

    if origin_matches(&actual, expected) {
        Ok(())
    } else {
        Err(reject("Origin/Referer does not match expected origin"))
    }
}

fn origin_matches(actual: &Url, expected: &Url) -> bool {
    actual.scheme() == expected.scheme()
        && actual.host_str() == expected.host_str()
        && actual.port_or_known_default() == expected.port_or_known_default()
}

fn reject(message: &str) -> HttpResponse {
    HttpResponse::BadRequest()
        .content_type("application/json")
        .body(format!(r#"{{"message":"{message}"}}"#))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn matching_origin_accepted() {
        let req = TestRequest::default()
            .insert_header((header::ORIGIN, "https://app.example.com"))
            .to_http_request();
        assert!(check_request_origin(&req, &url("https://app.example.com/")).is_ok());
    }

    #[test]
    fn referer_fallback_accepted_when_origin_absent() {
        let req = TestRequest::default()
            .insert_header((header::REFERER, "https://app.example.com/login"))
            .to_http_request();
        assert!(check_request_origin(&req, &url("https://app.example.com/")).is_ok());
    }

    #[test]
    fn mismatched_host_rejected() {
        let req = TestRequest::default()
            .insert_header((header::ORIGIN, "https://evil.example.com"))
            .to_http_request();
        assert!(check_request_origin(&req, &url("https://app.example.com/")).is_err());
    }

    #[test]
    fn mismatched_scheme_rejected() {
        let req = TestRequest::default()
            .insert_header((header::ORIGIN, "http://app.example.com"))
            .to_http_request();
        assert!(check_request_origin(&req, &url("https://app.example.com/")).is_err());
    }

    #[test]
    fn mismatched_port_rejected() {
        let req = TestRequest::default()
            .insert_header((header::ORIGIN, "https://app.example.com:8443"))
            .to_http_request();
        assert!(check_request_origin(&req, &url("https://app.example.com/")).is_err());
    }

    #[test]
    fn missing_both_headers_rejected() {
        let req = TestRequest::default().to_http_request();
        assert!(check_request_origin(&req, &url("https://app.example.com/")).is_err());
    }

    #[test]
    fn unparseable_origin_rejected() {
        let req = TestRequest::default()
            .insert_header((header::ORIGIN, "not-a-url"))
            .to_http_request();
        assert!(check_request_origin(&req, &url("https://app.example.com/")).is_err());
    }
}
