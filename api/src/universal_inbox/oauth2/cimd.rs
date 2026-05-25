//! Client ID Metadata Discovery (CIMD).
//!
//! Implementation of `draft-ietf-oauth-client-id-metadata-document-00/-01`
//! (with MCP 2025-11-25 §Authorization restrictions applied):
//!
//! - A client whose `client_id` is an `https://` URL with a path component is
//!   a CIMD client. The AS fetches that URL and treats the returned JSON as
//!   the client's metadata.
//! - The fetched document must contain `client_id` equal to the URL itself
//!   and a non-empty `redirect_uris` array. Symmetric-secret auth methods are
//!   forbidden — CIMD is for public clients only.
//!
//! Security guards on the fetch:
//!
//! - HTTPS only, no redirect follow, response body capped (default 5 KB).
//! - SSRF protection: the URL's hostname is resolved up-front and every
//!   resolved IP must be a public unicast address. The resolved address is
//!   then pinned via `reqwest`'s `.resolve()` so the connect step cannot
//!   re-resolve to a private address (DNS rebinding).
//! - `application/json` content-type only.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use anyhow::Context;
use reqwest::redirect;
use ring::digest;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{configuration::CimdSettings, universal_inbox::UniversalInboxError};

/// Convenience constructor: most CIMD failures are "input from the client
/// (its client_id URL) was invalid", so they share a structured shape.
fn invalid(user_error: impl Into<String>) -> UniversalInboxError {
    UniversalInboxError::InvalidInputData {
        source: None,
        user_error: user_error.into(),
    }
}

/// Parsed-and-validated CIMD document. Mirrors the IANA-registered RFC 7591
/// client metadata fields used by MCP CIMD clients. Forbidden fields
/// (`client_secret`, symmetric `token_endpoint_auth_method` values) are
/// rejected at parse time — see [`ClientMetadataDocument::validate`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientMetadataDocument {
    pub client_id: String,
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub grant_types: Vec<String>,
    #[serde(default)]
    pub response_types: Vec<String>,
    pub token_endpoint_auth_method: Option<String>,
    pub scope: Option<String>,
    pub client_uri: Option<String>,
    pub logo_uri: Option<String>,
    pub tos_uri: Option<String>,
    pub policy_uri: Option<String>,
    pub software_id: Option<String>,
    pub software_version: Option<String>,
    pub application_type: Option<String>,
    pub jwks_uri: Option<String>,
    pub jwks: Option<serde_json::Value>,
}

/// Result of a successful CIMD fetch.
#[derive(Debug, Clone)]
pub struct FetchedClientMetadata {
    pub document: ClientMetadataDocument,
    pub body_sha256: Vec<u8>,
    pub ttl: Duration,
}

/// Returns true iff `client_id` is a syntactically-valid CIMD identifier:
/// an `https://` URL with a non-empty path component. Opaque (UUID, etc.)
/// client_ids that come from RFC 7591 DCR registration are not CIMD.
pub fn is_cimd_client_id(client_id: &str) -> bool {
    let Ok(parsed) = Url::parse(client_id) else {
        return false;
    };
    parsed.scheme() == "https" && parsed.path() != "/" && !parsed.path().is_empty()
}

impl ClientMetadataDocument {
    /// Validate the cross-field invariants the IETF draft and MCP spec
    /// require. The caller has already deserialized the body — these are the
    /// checks that look at the *content* rather than the JSON shape.
    fn validate(&self, expected_client_id: &str) -> Result<(), UniversalInboxError> {
        // The `client_id` claim in the document MUST match the URL we fetched
        // it from — otherwise an attacker who controls *some* metadata URL
        // could claim to be a different client.
        if self.client_id != expected_client_id {
            return Err(invalid(format!(
                "CIMD document client_id ({}) does not match metadata URL ({expected_client_id})",
                self.client_id
            )));
        }

        if self.redirect_uris.is_empty() {
            return Err(invalid(
                "CIMD document must declare at least one redirect_uri",
            ));
        }
        for redirect_uri in &self.redirect_uris {
            super::service::validate_redirect_uri(redirect_uri)?;
        }

        // CIMD is the public-client path. Confidential-client auth methods
        // are explicitly forbidden by the draft (§5).
        if let Some(method) = self.token_endpoint_auth_method.as_deref() {
            const FORBIDDEN: &[&str] = &[
                "client_secret_basic",
                "client_secret_post",
                "client_secret_jwt",
            ];
            if FORBIDDEN.contains(&method) {
                return Err(invalid(format!(
                    "CIMD document uses forbidden token_endpoint_auth_method '{method}' \
                     (CIMD is for public clients only)"
                )));
            }
        }

        Ok(())
    }
}

/// Fetch a CIMD document and validate it.
///
/// `url` is expected to have already passed [`is_cimd_client_id`]. The
/// returned [`FetchedClientMetadata`] carries the canonical body hash and
/// the TTL the caller should persist in the metadata cache row.
pub async fn fetch_and_validate(
    url: &str,
    settings: &CimdSettings,
) -> Result<FetchedClientMetadata, UniversalInboxError> {
    let parsed_url =
        Url::parse(url).map_err(|err| invalid(format!("Invalid CIMD URL {url}: {err}")))?;
    if parsed_url.scheme() != "https" {
        return Err(invalid(format!("CIMD URL must be https://, got {url}")));
    }

    let host = parsed_url
        .host_str()
        .ok_or_else(|| invalid(format!("CIMD URL has no host: {url}")))?
        .to_string();
    let port = parsed_url.port_or_known_default().unwrap_or(443);

    // SSRF defense: resolve up-front and reject if any address is in a
    // special-use range. We then pin the resolved address via reqwest's
    // `.resolve()` so the connect step cannot re-resolve to a private
    // address (DNS rebinding).
    let safe_addr = resolve_safe(&host, port).await?;

    let pinned_client = build_pinned_client(&host, safe_addr, settings)?;
    let response = pinned_client
        .get(parsed_url.as_str())
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "universal-inbox-cimd/1.0")
        .send()
        .await
        .map_err(|err| invalid(format!("Failed to fetch CIMD document at {url}: {err}")))?;

    if response.status() != reqwest::StatusCode::OK {
        return Err(invalid(format!(
            "CIMD fetch of {url} returned HTTP {}, expected 200",
            response.status()
        )));
    }

    if let Some(content_type) = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        && !content_type
            .split(';')
            .next()
            .map(|s| s.trim().eq_ignore_ascii_case("application/json"))
            .unwrap_or(false)
    {
        return Err(invalid(format!(
            "CIMD document at {url} has non-JSON content-type '{content_type}'"
        )));
    }

    let max_body_bytes = settings.max_body_bytes;
    let cache_max_age = parse_cache_max_age(&response);
    let body = read_capped(response, max_body_bytes).await?;

    let document: ClientMetadataDocument = serde_json::from_slice(&body)
        .map_err(|err| invalid(format!("CIMD document at {url} is not valid JSON: {err}")))?;

    // Detect forbidden top-level fields by parsing the same body as a
    // free-form JSON object too — `client_secret`/`client_secret_expires_at`
    // would otherwise be silently dropped by the typed struct.
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body)
        && let Some(obj) = value.as_object()
    {
        for forbidden in ["client_secret", "client_secret_expires_at"] {
            if obj.contains_key(forbidden) {
                return Err(invalid(format!(
                    "CIMD document at {url} contains forbidden field '{forbidden}' \
                     (symmetric client credentials are not allowed)"
                )));
            }
        }
    }

    document.validate(url)?;

    let body_sha256 = digest::digest(&digest::SHA256, &body).as_ref().to_vec();
    let ttl = clamp_ttl(cache_max_age, settings);

    Ok(FetchedClientMetadata {
        document,
        body_sha256,
        ttl,
    })
}

/// Resolve `host:port` and return the first SocketAddr whose IP is a public
/// unicast address. Rejects the entire fetch if *any* resolved address is in
/// a special-use range — picking only the "good" addresses would still let
/// an attacker route the response based on which IP we connect to.
async fn resolve_safe(host: &str, port: u16) -> Result<SocketAddr, UniversalInboxError> {
    // Reject IP literals as hostnames entirely. CIMD docs live at registered
    // domain names; a bare IP as `client_id` host has no legitimate use and
    // is overwhelmingly an SSRF probe.
    if host.parse::<IpAddr>().is_ok() {
        return Err(invalid(format!(
            "CIMD URL host must be a domain name, got IP literal {host}"
        )));
    }

    let lookup = tokio::net::lookup_host((host, port))
        .await
        .map_err(|err| invalid(format!("Failed to resolve CIMD host {host}: {err}")))?;

    let mut chosen: Option<SocketAddr> = None;
    for addr in lookup {
        if !is_safe_public_ip(addr.ip()) {
            return Err(invalid(format!(
                "CIMD host {host} resolves to a special-use address ({}); refusing to fetch",
                addr.ip()
            )));
        }
        if chosen.is_none() {
            chosen = Some(addr);
        }
    }

    chosen.ok_or_else(|| invalid(format!("CIMD host {host} did not resolve to any address")))
}

fn build_pinned_client(
    host: &str,
    addr: SocketAddr,
    settings: &CimdSettings,
) -> Result<reqwest::Client, UniversalInboxError> {
    reqwest::Client::builder()
        .redirect(redirect::Policy::none())
        .timeout(Duration::from_secs(settings.timeout_secs))
        .connect_timeout(Duration::from_secs(2))
        .pool_max_idle_per_host(2)
        .resolve(host, addr)
        .https_only(true)
        .build()
        .context("Failed to build CIMD HTTP client")
        .map_err(UniversalInboxError::Unexpected)
}

async fn read_capped(
    response: reqwest::Response,
    cap: usize,
) -> Result<Vec<u8>, UniversalInboxError> {
    use futures::StreamExt;

    let mut buf: Vec<u8> = Vec::with_capacity(cap.min(4096));
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|err| invalid(format!("CIMD response body stream error: {err}")))?;
        if buf.len() + chunk.len() > cap {
            return Err(invalid(format!(
                "CIMD document exceeds {cap}-byte size cap"
            )));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn parse_cache_max_age(response: &reqwest::Response) -> Option<u64> {
    let header = response
        .headers()
        .get(reqwest::header::CACHE_CONTROL)?
        .to_str()
        .ok()?;
    for directive in header.split(',') {
        let directive = directive.trim();
        if let Some(value) = directive.strip_prefix("max-age=")
            && let Ok(parsed) = value.trim().parse::<u64>()
        {
            return Some(parsed);
        }
    }
    None
}

fn clamp_ttl(cache_max_age: Option<u64>, settings: &CimdSettings) -> Duration {
    let raw = cache_max_age.unwrap_or(settings.default_ttl_secs);
    let clamped = raw.clamp(settings.min_ttl_secs, settings.max_ttl_secs);
    Duration::from_secs(clamped)
}

/// Returns `true` when `addr` is a routable public unicast address. Anything
/// in an RFC 6890 / IANA special-use range — loopback, private, link-local,
/// CGNAT, multicast, broadcast, documentation, ULA, etc. — returns `false`.
fn is_safe_public_ip(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => is_safe_public_ipv4(v4),
        IpAddr::V6(v6) => is_safe_public_ipv6(v6),
    }
}

fn is_safe_public_ipv4(addr: Ipv4Addr) -> bool {
    if addr.is_loopback()
        || addr.is_private()
        || addr.is_link_local()
        || addr.is_multicast()
        || addr.is_broadcast()
        || addr.is_unspecified()
        || addr.is_documentation()
    {
        return false;
    }
    let octets = addr.octets();
    // CGNAT 100.64.0.0/10
    if octets[0] == 100 && (octets[1] & 0xc0) == 64 {
        return false;
    }
    // 0.0.0.0/8 — "this network"
    if octets[0] == 0 {
        return false;
    }
    // Benchmarking 198.18.0.0/15
    if octets[0] == 198 && (octets[1] & 0xfe) == 18 {
        return false;
    }
    // IETF protocol assignments 192.0.0.0/24
    if octets[0] == 192 && octets[1] == 0 && octets[2] == 0 {
        return false;
    }
    // Reserved 240.0.0.0/4 (except broadcast handled above)
    if octets[0] >= 240 {
        return false;
    }
    true
}

fn is_safe_public_ipv6(addr: Ipv6Addr) -> bool {
    if addr.is_loopback() || addr.is_unspecified() || addr.is_multicast() {
        return false;
    }
    let seg = addr.segments();
    // Unique-local fc00::/7
    if (seg[0] & 0xfe00) == 0xfc00 {
        return false;
    }
    // Link-local fe80::/10
    if (seg[0] & 0xffc0) == 0xfe80 {
        return false;
    }
    // Site-local (deprecated) fec0::/10
    if (seg[0] & 0xffc0) == 0xfec0 {
        return false;
    }
    // Documentation 2001:db8::/32
    if seg[0] == 0x2001 && seg[1] == 0x0db8 {
        return false;
    }
    // IPv4-mapped / IPv4-compatible ::ffff:0:0/96 and ::/96 — re-check against v4 rules
    if seg[0] == 0
        && seg[1] == 0
        && seg[2] == 0
        && seg[3] == 0
        && seg[4] == 0
        && (seg[5] == 0 || seg[5] == 0xffff)
    {
        let v4 = Ipv4Addr::new(
            (seg[6] >> 8) as u8,
            (seg[6] & 0xff) as u8,
            (seg[7] >> 8) as u8,
            (seg[7] & 0xff) as u8,
        );
        return is_safe_public_ipv4(v4);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cimd_client_id_recognized() {
        assert!(is_cimd_client_id(
            "https://claude.ai/oauth/claude-code-client-metadata"
        ));
        assert!(is_cimd_client_id("https://example.com/client.json"));
    }

    #[test]
    fn cimd_client_id_rejects_opaque_and_http() {
        assert!(!is_cimd_client_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_cimd_client_id("http://example.com/client.json"));
        assert!(!is_cimd_client_id("https://example.com/"));
        assert!(!is_cimd_client_id("https://example.com"));
        assert!(!is_cimd_client_id("not-a-url"));
    }

    #[test]
    fn rejects_ipv4_private() {
        assert!(!is_safe_public_ipv4(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(!is_safe_public_ipv4(Ipv4Addr::new(172, 16, 5, 4)));
        assert!(!is_safe_public_ipv4(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!is_safe_public_ipv4(Ipv4Addr::new(127, 0, 0, 1)));
        assert!(!is_safe_public_ipv4(Ipv4Addr::new(169, 254, 169, 254)));
        assert!(!is_safe_public_ipv4(Ipv4Addr::new(100, 64, 0, 1))); // CGNAT
        assert!(!is_safe_public_ipv4(Ipv4Addr::new(0, 0, 0, 0)));
        assert!(!is_safe_public_ipv4(Ipv4Addr::new(255, 255, 255, 255)));
    }

    #[test]
    fn accepts_ipv4_public() {
        assert!(is_safe_public_ipv4(Ipv4Addr::new(1, 1, 1, 1)));
        assert!(is_safe_public_ipv4(Ipv4Addr::new(8, 8, 8, 8)));
    }

    #[test]
    fn rejects_ipv6_special() {
        assert!(!is_safe_public_ipv6(Ipv6Addr::LOCALHOST));
        assert!(!is_safe_public_ipv6(Ipv6Addr::UNSPECIFIED));
        // ULA
        assert!(!is_safe_public_ipv6("fd00::1".parse::<Ipv6Addr>().unwrap()));
        // link-local
        assert!(!is_safe_public_ipv6("fe80::1".parse::<Ipv6Addr>().unwrap()));
        // IPv4-mapped private
        assert!(!is_safe_public_ipv6(
            "::ffff:10.0.0.1".parse::<Ipv6Addr>().unwrap()
        ));
    }

    #[test]
    fn document_validate_rejects_mismatched_client_id() {
        let doc = ClientMetadataDocument {
            client_id: "https://evil.example/client.json".to_string(),
            client_name: None,
            redirect_uris: vec!["https://evil.example/cb".to_string()],
            grant_types: vec![],
            response_types: vec![],
            token_endpoint_auth_method: None,
            scope: None,
            client_uri: None,
            logo_uri: None,
            tos_uri: None,
            policy_uri: None,
            software_id: None,
            software_version: None,
            application_type: None,
            jwks_uri: None,
            jwks: None,
        };
        let err = doc
            .validate("https://legit.example/client.json")
            .unwrap_err();
        assert!(matches!(err, UniversalInboxError::InvalidInputData { .. }));
    }

    #[test]
    fn document_validate_rejects_empty_redirect_uris() {
        let doc = ClientMetadataDocument {
            client_id: "https://example.com/client.json".to_string(),
            client_name: None,
            redirect_uris: vec![],
            grant_types: vec![],
            response_types: vec![],
            token_endpoint_auth_method: None,
            scope: None,
            client_uri: None,
            logo_uri: None,
            tos_uri: None,
            policy_uri: None,
            software_id: None,
            software_version: None,
            application_type: None,
            jwks_uri: None,
            jwks: None,
        };
        assert!(matches!(
            doc.validate("https://example.com/client.json").unwrap_err(),
            UniversalInboxError::InvalidInputData { .. }
        ));
    }

    #[test]
    fn document_validate_rejects_symmetric_auth_method() {
        let doc = ClientMetadataDocument {
            client_id: "https://example.com/client.json".to_string(),
            client_name: None,
            redirect_uris: vec!["https://example.com/cb".to_string()],
            grant_types: vec![],
            response_types: vec![],
            token_endpoint_auth_method: Some("client_secret_basic".to_string()),
            scope: None,
            client_uri: None,
            logo_uri: None,
            tos_uri: None,
            policy_uri: None,
            software_id: None,
            software_version: None,
            application_type: None,
            jwks_uri: None,
            jwks: None,
        };
        assert!(matches!(
            doc.validate("https://example.com/client.json").unwrap_err(),
            UniversalInboxError::InvalidInputData { .. }
        ));
    }
}
