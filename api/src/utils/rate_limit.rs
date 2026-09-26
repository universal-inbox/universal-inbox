//! Shared helpers for governor-based per-IP rate limiting.
//!
//! Three Actix routes use the same pattern (real-IP keying with reject-on-
//! unidentifiable): `/ping`, the OAuth2 token /
//! register endpoints, and the auth-state-changing user endpoints.
//! Extracted here so the IP-resolution and check semantics
//! cannot drift between call sites.
//!
//! **Why reject-on-unspecified rather than bucket-into-unspecified:**
//! `realip_remote_addr` may yield `0.0.0.0` / `::` if a proxy header is
//! malformed or absent, or the request cannot be otherwise pinned to a
//! caller. Bucketing every such request under the unspecified address turns
//! the limiter into a global throttle for unidentified traffic, which is
//! both unfair (one malformed-header bot DoSes everyone) and ineffective (an
//! attacker who strips the header floods the unspecified bucket while
//! legitimate clients keep their own buckets). Refusing with `400 Bad
//! Request` matches the OAuth2 limiter.

use std::net::IpAddr;

use actix_web::{HttpRequest, HttpResponse, http::header::HeaderMap, web};
use governor::{RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use tracing::warn;

use crate::configuration::{DEFAULT_TRUSTED_PROXY_HOPS, Settings};

/// Above this many tracked addresses, stale buckets are evicted: the keyed
/// store never shrinks on its own, and every distinct address adds an entry.
const MAX_TRACKED_KEYS_BEFORE_CLEANUP: usize = 10_000;

/// A keyed governor rate limiter scoped on the caller's real IP.
pub type IpRateLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

/// Check whether the given request fits within the per-IP budget.
///
/// Returns `Ok(())` to proceed, or an `Err(HttpResponse)` ready to be
/// returned to the client:
/// - `429 Too Many Requests` when the per-IP bucket is exhausted
/// - `400 Bad Request` when the real client IP cannot be determined
///   (unparseable header, unspecified address); see the module-level note
///   for why we refuse rather than bucket
pub fn check_ip_rate_limit(
    req: &HttpRequest,
    rate_limiter: &IpRateLimiter,
) -> Result<(), Box<HttpResponse>> {
    let Some(ip) = resolve_client_ip(req) else {
        // 400 Bad Request per RFC 6585 reasoning: 429 means "too many
        // requests"; here we cannot even identify the caller, so reuse the
        // OAuth2 limiter's behaviour and refuse with a generic 400 JSON
        // envelope.
        return Err(Box::new(
            HttpResponse::BadRequest()
                .content_type("application/json")
                .body(r#"{"message":"Unable to determine client IP for rate limiting"}"#),
        ));
    };
    if rate_limiter.len() > MAX_TRACKED_KEYS_BEFORE_CLEANUP {
        rate_limiter.retain_recent();
        rate_limiter.shrink_to_fit();
    }
    if rate_limiter.check_key(&ip).is_err() {
        return Err(Box::new(HttpResponse::TooManyRequests().finish()));
    }
    Ok(())
}

/// Returns the client IP the rate limiters are keyed on, if it can be parsed
/// AND is a specified address (not `0.0.0.0` / `::`).
///
/// It used to come from `ConnectionInfo::realip_remote_addr()`, which trusts
/// the *first* entry of the client-supplied `Forwarded` / `X-Forwarded-For`
/// headers: any caller could pick a fresh bucket per request and bypass every
/// limit. The address is now taken from the TCP peer, or, when
/// `application.security.trusted_proxy_hops` declares N trusted proxies, from
/// the N-th entry from the right of `X-Forwarded-For` (the one appended by the
/// outermost trusted proxy). See [`client_ip_from`].
///
/// Returns `None` for absent, unparseable, or wildcard addresses so the
/// caller can refuse the request rather than collapse every unidentifiable
/// client into the unspecified bucket. See the module-level note.
pub fn resolve_client_ip(req: &HttpRequest) -> Option<IpAddr> {
    let trusted_proxy_hops = req
        .app_data::<web::Data<Settings>>()
        .map(|settings| settings.application.security.trusted_proxy_hops)
        .unwrap_or(DEFAULT_TRUSTED_PROXY_HOPS);
    let forwarded_for = forwarded_for_chain(req.headers());
    if let Some(chain) = forwarded_for.as_deref()
        && trusted_proxy_hops > 0
    {
        let entries = chain.split(',').count();
        if entries < trusted_proxy_hops {
            // Most likely a misconfiguration rather than an attack: hint at
            // the fix without logging the (personal) addresses themselves.
            warn!(
                x_forwarded_for.entries = entries,
                trusted_proxy_hops,
                x_forwarded_for.masked = %mask_forwarded_for(chain),
                "X-Forwarded-For has fewer entries than application.security.trusted_proxy_hops: \
                 request refused; lower trusted_proxy_hops to the number of proxies in front of the API"
            );
        }
    }
    client_ip_from(
        req.peer_addr().map(|addr| addr.ip()),
        forwarded_for.as_deref(),
        trusted_proxy_hops,
    )
}

/// Every `X-Forwarded-For` header of the request joined into one chain, or
/// `None` when there is none.
pub fn forwarded_for_chain(headers: &HeaderMap) -> Option<String> {
    let chain = headers
        .get_all("x-forwarded-for")
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join(",");
    (!chain.is_empty()).then_some(chain)
}

/// The `X-Forwarded-For` chain with its public addresses replaced by
/// `public` (and unparseable entries by `invalid`), for tracing.
///
/// Public entries are client addresses, i.e. personal data that must not be
/// exported (see `observability::redaction`). Loopback, private and
/// link-local entries are the operator's own proxies and are kept: together
/// with the entry count they show how many proxies append to the header,
/// which is the value `application.security.trusted_proxy_hops` needs.
pub fn mask_forwarded_for(chain: &str) -> String {
    chain
        .split(',')
        .map(str::trim)
        .map(|entry| match parse_remote_addr(entry) {
            Some(ip) if is_internal_address(&ip) => ip.to_string(),
            Some(_) => "public".to_string(),
            None => "invalid".to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Loopback, private (RFC 1918 / unique local) and link-local addresses:
/// the ones a reverse proxy in the operator's own network would have.
fn is_internal_address(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

/// Pick the client IP from the TCP peer and the `X-Forwarded-For` chain.
///
/// With `trusted_proxy_hops == 0`, or without the header, the peer address
/// is used. Otherwise each trusted proxy appended one entry, so the client is
/// the `trusted_proxy_hops`-th entry from the right; a chain shorter than
/// that cannot have come through all the trusted proxies and is refused.
pub fn client_ip_from(
    peer_ip: Option<IpAddr>,
    forwarded_for: Option<&str>,
    trusted_proxy_hops: usize,
) -> Option<IpAddr> {
    let ip = match forwarded_for {
        Some(chain) if trusted_proxy_hops > 0 => {
            let hops: Vec<&str> = chain.split(',').map(str::trim).collect();
            let index = hops.len().checked_sub(trusted_proxy_hops)?;
            parse_remote_addr(hops[index])?
        }
        _ => peer_ip?,
    };
    (!ip.is_unspecified()).then_some(ip)
}

/// Parse an address string that may be a bare IP, `ipv4:port`, or `[ipv6]:port`.
///
/// In actix-web 4.x, `ConnectionInfo::realip_remote_addr` already returns a
/// bare IP in every documented case — `peer_addr` is `addr.ip().to_string()`,
/// `X-Forwarded-For` is bare per spec, and `Forwarded` runs through
/// `bare_address` which strips brackets and ports. So a plain
/// `raw.parse::<IpAddr>()` succeeds on the common path. We still tolerate the
/// `host:port` and `[ipv6]:port` forms defensively in case a future actix
/// version or a custom extractor hands us a socket-address-shaped value.
///
/// The earlier `raw.rsplit_once(':')` shortcut was unsafe: it stripped the
/// last `:nnn` group of every IPv6 address (`::1` → `:`, `2001:db8::1` →
/// `2001:db8:`), so behind a proxy that set `X-Forwarded-For` to an IPv6
/// loopback / link-local / GUA the parser failed and `check_ip_rate_limit`
/// returned 400 to every caller.
fn parse_remote_addr(raw: &str) -> Option<IpAddr> {
    // Bare IPv4 / IPv6 — try first because IPv6 contains internal colons
    // that the port-stripping branches below would corrupt.
    if let Ok(ip) = raw.parse::<IpAddr>() {
        return Some(ip);
    }
    // Bracketed IPv6 with optional port: `[2001:db8::1]:8080` or `[::1]`.
    // Require a closing bracket so `[::1` doesn't slip through as `::1`.
    if let Some(rest) = raw.strip_prefix('[') {
        let bare = if let Some((b, _port)) = rest.split_once("]:") {
            b
        } else {
            rest.strip_suffix(']')?
        };
        return bare.parse::<IpAddr>().ok();
    }
    // IPv4 with port: `192.0.2.1:8080`. Only treat as `host:port` when there
    // is exactly one colon — any other count is an IPv6 address we already
    // failed to parse and must not mangle further.
    if raw.matches(':').count() == 1
        && let Some((addr, _port)) = raw.rsplit_once(':')
    {
        return addr.parse::<IpAddr>().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_forwarded_for_hides_public_addresses_only() {
        assert_eq!(
            mask_forwarded_for("203.0.113.7, 10.0.3.7,172.18.0.2 , ::1, fd00::1, not-an-ip"),
            "public, 10.0.3.7, 172.18.0.2, ::1, fd00::1, invalid"
        );
        assert_eq!(mask_forwarded_for("2001:db8::1"), "public");
    }

    #[test]
    fn forwarded_for_chain_joins_repeated_headers() {
        let mut headers = HeaderMap::new();
        assert_eq!(forwarded_for_chain(&headers), None);
        headers.append(
            actix_web::http::header::HeaderName::from_static("x-forwarded-for"),
            actix_web::http::header::HeaderValue::from_static("203.0.113.7"),
        );
        headers.append(
            actix_web::http::header::HeaderName::from_static("x-forwarded-for"),
            actix_web::http::header::HeaderValue::from_static("10.0.0.1"),
        );
        assert_eq!(
            forwarded_for_chain(&headers).as_deref(),
            Some("203.0.113.7,10.0.0.1")
        );
    }

    #[test]
    fn parse_remote_addr_handles_bare_ipv4() {
        assert_eq!(
            parse_remote_addr("127.0.0.1"),
            Some(IpAddr::from([127, 0, 0, 1]))
        );
        assert_eq!(
            parse_remote_addr("203.0.113.7"),
            Some(IpAddr::from([203, 0, 113, 7]))
        );
    }

    #[test]
    fn parse_remote_addr_handles_ipv4_with_port() {
        assert_eq!(
            parse_remote_addr("127.0.0.1:8080"),
            Some(IpAddr::from([127, 0, 0, 1]))
        );
    }

    #[test]
    fn parse_remote_addr_handles_bare_ipv6() {
        assert_eq!(
            parse_remote_addr("::1"),
            Some(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]))
        );
        assert_eq!(
            parse_remote_addr("2001:db8::1"),
            Some("2001:db8::1".parse().unwrap())
        );
        assert_eq!(
            parse_remote_addr("fe80::1"),
            Some("fe80::1".parse().unwrap())
        );
    }

    #[test]
    fn parse_remote_addr_handles_bracketed_ipv6_with_port() {
        assert_eq!(
            parse_remote_addr("[::1]:8080"),
            Some(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]))
        );
        assert_eq!(
            parse_remote_addr("[2001:db8::1]:443"),
            Some("2001:db8::1".parse().unwrap())
        );
    }

    fn ip(raw: &str) -> IpAddr {
        raw.parse().unwrap()
    }

    #[test]
    fn client_ip_uses_peer_without_trusted_proxy() {
        assert_eq!(
            client_ip_from(Some(ip("198.51.100.1")), Some("203.0.113.9"), 0),
            Some(ip("198.51.100.1"))
        );
    }

    #[test]
    fn client_ip_ignores_client_supplied_forwarded_entries() {
        // The client sent `X-Forwarded-For: 203.0.113.9`; the single trusted
        // proxy appended the real address.
        assert_eq!(
            client_ip_from(Some(ip("10.0.0.2")), Some("203.0.113.9, 198.51.100.1"), 1),
            Some(ip("198.51.100.1"))
        );
        assert_eq!(
            client_ip_from(
                Some(ip("10.0.0.2")),
                Some("203.0.113.9, 198.51.100.1, 10.0.0.3"),
                2
            ),
            Some(ip("198.51.100.1"))
        );
    }

    #[test]
    fn client_ip_falls_back_to_peer_without_header() {
        assert_eq!(
            client_ip_from(Some(ip("198.51.100.1")), None, 1),
            Some(ip("198.51.100.1"))
        );
    }

    #[test]
    fn client_ip_refuses_short_chain_and_unspecified() {
        assert_eq!(
            client_ip_from(Some(ip("10.0.0.2")), Some("198.51.100.1"), 2),
            None
        );
        assert_eq!(client_ip_from(Some(ip("0.0.0.0")), None, 0), None);
        assert_eq!(
            client_ip_from(Some(ip("10.0.0.2")), Some("0.0.0.0"), 1),
            None
        );
    }

    #[test]
    fn parse_remote_addr_rejects_garbage() {
        assert_eq!(parse_remote_addr(""), None);
        assert_eq!(parse_remote_addr("not-an-ip"), None);
        assert_eq!(parse_remote_addr("1.2.3"), None);
        assert_eq!(parse_remote_addr("[::1"), None);
        // Two comma-separated values (raw X-Forwarded-For chain) — caller is
        // expected to hand us the first hop only; reject anything else.
        assert_eq!(parse_remote_addr("1.2.3.4, 5.6.7.8"), None);
    }
}
