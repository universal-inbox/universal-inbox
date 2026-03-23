use actix_web::{HttpResponse, web};
use serde_json::json;

use crate::configuration::Settings;

/// RFC 9728: OAuth 2.0 Protected Resource Metadata
/// GET /.well-known/oauth-protected-resource
pub async fn protected_resource_metadata(settings: web::Data<Settings>) -> HttpResponse {
    let base_url = settings
        .application
        .front_base_url
        .as_str()
        .trim_end_matches('/');
    let api_path = &settings.application.api_path;
    let resource = format!("{base_url}{api_path}mcp");

    HttpResponse::Ok().json(json!({
        "resource": resource,
        "authorization_servers": [base_url],
        "bearer_methods_supported": ["header"],
        "scopes_supported": ["read", "write"],
        "resource_documentation": "https://doc.universal-inbox.com"
    }))
}

/// RFC 8414: OAuth 2.0 Authorization Server Metadata
/// GET /.well-known/oauth-authorization-server
pub async fn authorization_server_metadata(settings: web::Data<Settings>) -> HttpResponse {
    let base_url = settings
        .application
        .front_base_url
        .as_str()
        .trim_end_matches('/');
    let api_path = &settings.application.api_path;

    HttpResponse::Ok().json(json!({
        "issuer": base_url,
        "authorization_endpoint": format!("{base_url}{api_path}oauth2/authorize"),
        "token_endpoint": format!("{base_url}{api_path}oauth2/token"),
        "registration_endpoint": format!("{base_url}{api_path}oauth2/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": ["read", "write"],
        "resource_indicators_supported": true
    }))
}
