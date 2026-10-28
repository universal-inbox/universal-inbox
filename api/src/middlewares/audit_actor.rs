use actix_web::{
    Error, HttpMessage,
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    http::header::HeaderMap,
    middleware::Next,
};

use universal_inbox::audit::{AuditActor, AuditUserChannel};

use crate::{
    middlewares::jwt_auth::{AuthSource, Authenticated},
    universal_inbox::audit::actor_context::with_actor,
    utils::jwt::Claims,
};

/// Header identifying a first-party client, eg. `extension/1.4.2` for the browser extension.
pub const CLIENT_HEADER: &str = "x-universal-inbox-client";

/// Middleware running authenticated requests with their audit actor (see
/// `universal_inbox::audit::actor_context`), recorded with the notification and task changes
/// they make.
///
/// Must run after [`super::jwt_auth::AuthenticateMiddleware`], which injects the
/// [`Authenticated`] claims it reads. Unauthenticated requests run without an actor.
pub async fn audit_actor_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let actor = req
        .extensions()
        .get::<Authenticated<Claims>>()
        .map(|authenticated| request_actor(authenticated, req.headers()));

    match actor {
        Some(actor) => with_actor(actor, next.call(req)).await,
        None => next.call(req).await,
    }
}

fn request_actor(authenticated: &Authenticated<Claims>, headers: &HeaderMap) -> AuditActor {
    // OAuth tokens carry an audience: only the MCP endpoint accepts them.
    if authenticated.claims.aud.is_some() {
        return AuditActor::Mcp {
            client_id: authenticated.claims.client_id.clone().unwrap_or_default(),
        };
    }

    if let Some(version) = headers
        .get(CLIENT_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|client| client.strip_prefix("extension/"))
    {
        return AuditActor::Extension {
            version: version.chars().take(32).collect(),
        };
    }

    match authenticated.source {
        AuthSource::Bearer => AuditActor::ApiKey,
        AuthSource::Session => AuditActor::User {
            via: AuditUserChannel::Web,
        },
    }
}

#[cfg(test)]
mod tests {
    use actix_web::http::header::{HeaderName, HeaderValue};
    use pretty_assertions::assert_eq;

    use crate::middlewares::jwt_auth::JWT;

    use super::*;

    fn authenticated(source: AuthSource, aud: Option<&str>) -> Authenticated<Claims> {
        Authenticated {
            jwt: JWT("token".to_string()),
            claims: Claims {
                exp: 0,
                iat: 0,
                sub: "user".to_string(),
                jti: "jti".to_string(),
                purpose: None,
                aud: aud.map(str::to_string),
                scope: None,
                client_id: aud.map(|_| "mcp-client".to_string()),
            },
            source,
        }
    }

    fn headers(client: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(client) = client {
            headers.insert(
                HeaderName::from_static(CLIENT_HEADER),
                HeaderValue::from_str(client).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn test_request_actor() {
        assert_eq!(
            request_actor(&authenticated(AuthSource::Session, None), &headers(None)),
            AuditActor::User {
                via: AuditUserChannel::Web
            }
        );
        assert_eq!(
            request_actor(&authenticated(AuthSource::Bearer, None), &headers(None)),
            AuditActor::ApiKey
        );
        assert_eq!(
            request_actor(
                &authenticated(AuthSource::Bearer, None),
                &headers(Some("extension/1.4.2"))
            ),
            AuditActor::Extension {
                version: "1.4.2".to_string()
            }
        );
        assert_eq!(
            request_actor(
                &authenticated(AuthSource::Bearer, None),
                &headers(Some("other/1.0"))
            ),
            AuditActor::ApiKey
        );
        assert_eq!(
            request_actor(
                &authenticated(AuthSource::Bearer, Some("https://mcp")),
                &headers(Some("extension/1.4.2"))
            ),
            AuditActor::Mcp {
                client_id: "mcp-client".to_string()
            }
        );
    }
}
