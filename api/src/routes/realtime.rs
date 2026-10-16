//! Server-sent event stream of the realtime events of the authenticated user.

use std::{sync::Arc, time::Duration};

use actix_web::{
    HttpResponse, Scope,
    http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_TYPE},
    web,
};

use crate::{
    configuration::Settings,
    middlewares::jwt_auth::{AuthSource, Authenticated},
    universal_inbox::{
        UniversalInboxError,
        realtime::{
            hub::EventHub,
            stream::{StreamSession, open_event_stream},
        },
        user::service::UserService,
    },
    utils::{
        jwt::Claims,
        rate_limit::{UserRateLimiters, check_user_rate_limit},
    },
};

/// Path of the stream in the API scope.
pub const EVENTS_PATH: &str = "/events";

pub fn scope() -> Scope {
    web::scope(EVENTS_PATH).service(
        web::resource("")
            .name("events")
            .route(web::get().to(stream_events)),
    )
}

pub async fn stream_events(
    authenticated: Authenticated<Claims>,
    settings: web::Data<Settings>,
    event_hub: web::Data<Arc<EventHub>>,
    user_service: web::Data<Arc<UserService>>,
    user_rate_limiters: web::Data<UserRateLimiters>,
) -> Result<HttpResponse, UniversalInboxError> {
    let realtime_settings = &settings.application.realtime;
    if !realtime_settings.enabled {
        // The web application keeps polling
        return Ok(HttpResponse::NotFound()
            .content_type("application/json")
            .body(r#"{"message": "Realtime events are disabled"}"#));
    }
    // An `EventSource` sends the session cookie only: API keys and OAuth tokens are not for
    // the web application
    if authenticated.source != AuthSource::Session {
        return Err(UniversalInboxError::Forbidden(
            "Realtime events require a session".to_string(),
        ));
    }
    let user_id = authenticated.user_id()?;
    check_user_rate_limit(&user_rate_limiters.events, user_id)?;

    let stream = open_event_stream(
        &event_hub,
        StreamSession {
            user_id,
            token_id: authenticated.claims.jti.clone(),
            issued_at: authenticated.claims.iat as i64,
            expires_at: authenticated.claims.exp as i64,
            validator: user_service.get_ref().clone(),
        },
        Duration::from_secs(realtime_settings.heartbeat_interval_in_seconds),
    )?;

    Ok(HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, "text/event-stream"))
        .insert_header((CACHE_CONTROL, "no-cache"))
        // Disable the buffering of reverse proxies (nginx) and the compression middleware
        .insert_header(("X-Accel-Buffering", "no"))
        .insert_header((CONTENT_ENCODING, "identity"))
        .insert_header((CONTENT_DISPOSITION, "inline"))
        .streaming(stream))
}
