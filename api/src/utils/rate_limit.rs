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

use actix_web::{HttpRequest, HttpResponse};
use governor::{RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};

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
) -> Result<(), HttpResponse> {
    let Some(ip) = resolve_client_ip(req) else {
        // 400 Bad Request per RFC 6585 reasoning: 429 means "too many
        // requests"; here we cannot even identify the caller, so reuse the
        // OAuth2 limiter's behaviour and refuse with a generic 400 JSON
        // envelope.
        return Err(HttpResponse::BadRequest()
            .content_type("application/json")
            .body(r#"{"message":"Unable to determine client IP for rate limiting"}"#));
    };
    if rate_limiter.check_key(&ip).is_err() {
        return Err(HttpResponse::TooManyRequests().finish());
    }
    Ok(())
}

/// Returns the real client IP from `ConnectionInfo::realip_remote_addr()` if
/// it can be parsed AND is a specified address (not `0.0.0.0` / `::`).
///
/// Returns `None` for absent, unparseable, or wildcard addresses so the
/// caller can refuse the request rather than collapse every unidentifiable
/// client into the unspecified bucket. See the module-level note.
pub fn resolve_client_ip(req: &HttpRequest) -> Option<IpAddr> {
    req.connection_info()
        .realip_remote_addr()
        .and_then(parse_remote_addr)
        .filter(|ip| !ip.is_unspecified())
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
