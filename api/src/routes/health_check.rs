use std::{net::IpAddr, num::NonZeroU32, sync::Arc};

use actix_web::{body::BoxBody, web, HttpRequest, HttpResponse};
use anyhow::Context;
use governor::{clock::DefaultClock, state::keyed::DefaultKeyedStateStore, Quota, RateLimiter};
use redis::AsyncCommands;
use serde_json::json;
use tokio::sync::RwLock;

use crate::{
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
    utils::cache::Cache,
};

/// Per-IP request budget for `/ping`.
///
/// 60 req/min is generous enough for typical uptime monitors (Pingdom,
/// UptimeRobot, Caddy upstream health checks…) which probe every 10-30s, but
/// tight enough to defeat the unauthenticated DoS amplifier.
const PING_RATE_LIMIT_PER_MINUTE: u32 = 60;

pub type PingRateLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

pub fn build_rate_limiter() -> Arc<PingRateLimiter> {
    let quota = Quota::per_minute(
        NonZeroU32::new(PING_RATE_LIMIT_PER_MINUTE)
            .expect("PING_RATE_LIMIT_PER_MINUTE must be non-zero"),
    );
    Arc::new(PingRateLimiter::keyed(quota))
}

/// Public, unauthenticated liveness probe.
///
/// **Security note:**
///
/// - The DB probe is `SELECT 1` issued directly against the pool (no
///   `Transaction`). sqlx checks out a connection, runs the query, and
///   returns the connection immediately. The previous implementation opened
///   a real `Transaction<'_, Postgres>` for every call, which held a pool
///   connection for the full round-trip and let an unauthenticated client
///   exhaust the connection pool with concurrent `/ping`s.
/// - The pool `Arc` is cloned out of `IntegrationConnectionService` while
///   holding the `RwLock` read guard for the minimum possible duration, then
///   the guard is dropped before the SQL roundtrip — `/ping` no longer
///   serializes against the integration-connection service write lock.
/// - A per-IP governor rate limit (60 req/min) is enforced using the real
///   client IP (`ConnectionInfo::realip_remote_addr` — `Forwarded` /
///   `X-Forwarded-For`) so that the limiter works correctly behind the
///   production Caddy proxy and does not bucket every client under
///   `peer_addr()` (the upstream IP).
pub async fn ping(
    req: HttpRequest,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    cache: web::Data<Cache>,
    rate_limiter: web::Data<Arc<PingRateLimiter>>,
) -> Result<HttpResponse, UniversalInboxError> {
    if let Some(response) = check_rate_limit(&req, &rate_limiter) {
        return Ok(response);
    }

    let cache_result: Result<String, anyhow::Error> = cache
        .connection_manager
        .clone()
        .ping()
        .await
        .context("Failed to ping Redis");

    // Clone the pool Arc out of the service while holding the read lock for
    // the minimum possible duration, then drop the guard before issuing the
    // SQL query so `/ping` does not contend with writers on the
    // integration-connection service.
    let pool = {
        let service = integration_connection_service.read().await;
        service.pool()
    };

    let db_result = sqlx::query_scalar!("SELECT 1")
        .fetch_one(&*pool)
        .await
        .map_err(|err| {
            let message = format!("Failed to ping database: {}", err);
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        });

    let mut response = if cache_result.is_err() || db_result.is_err() {
        HttpResponse::InternalServerError()
    } else {
        HttpResponse::Ok()
    };

    Ok(response.content_type("application/json").body(BoxBody::new(
        json!({
            "cache": cache_result.map(|_| "healthy").unwrap_or("unhealthy"),
            "database": db_result.map(|_| "healthy").unwrap_or("unhealthy"),
        })
        .to_string(),
    )))
}

fn check_rate_limit(req: &HttpRequest, rate_limiter: &PingRateLimiter) -> Option<HttpResponse> {
    // `realip_remote_addr()` honors the chain of `Forwarded` / `X-Forwarded-For`
    // headers (the actix-web default extractor) so that we bucket on the real
    // client IP rather than the upstream reverse-proxy IP. `peer_addr()` would
    // collapse every request behind Caddy into a single bucket and turn the
    // limiter into a deny-all.
    let ip = req
        .connection_info()
        .realip_remote_addr()
        .and_then(|raw| {
            // `realip_remote_addr` returns `host:port` for direct connections
            // and a bare IP for forwarded headers — strip an optional port.
            let trimmed = raw.rsplit_once(':').map(|(addr, _)| addr).unwrap_or(raw);
            trimmed.parse::<IpAddr>().ok()
        })
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    if rate_limiter.check_key(&ip).is_err() {
        return Some(HttpResponse::TooManyRequests().finish());
    }
    None
}
