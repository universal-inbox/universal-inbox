//! Transport implementations for the Model Context Protocol using actix-web.
//!
//! This module provides HTTP-based transport layers that enable MCP services
//! to communicate with clients over standard web protocols.
//!
//! ## Streamable HTTP
//!
//! The [`streamable_http_server`] module provides a bidirectional transport
//! with session management. This is ideal for:
//! - Full request/response communication patterns
//! - Maintaining client state across requests
//! - Complex interaction patterns
//! - Higher performance for bidirectional communication
//!
//! See [`StreamableHttpService`][crate::StreamableHttpService] for the main implementation.
//!
//! ## Framework-Level Composition
//!
//! The transport supports framework-level composition for mounting at custom paths
//! using a builder pattern:
//!
//! ```rust,no_run
//! use actix_web::{App, HttpServer, web};
//! use rmcp_actix_web::transport::StreamableHttpService;
//! use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
//! use std::{sync::Arc, time::Duration};
//!
//! # use rmcp::{ServerHandler, model::ServerInfo};
//! # #[derive(Clone)]
//! # struct MyService;
//! # impl ServerHandler for MyService {
//! #     fn get_info(&self) -> ServerInfo { ServerInfo::default() }
//! # }
//! # impl MyService { fn new() -> Self { Self } }
//! #[actix_web::main]
//! async fn main() -> std::io::Result<()> {
//!     // Create service OUTSIDE HttpServer::new() to share across workers
//!     let http_service = StreamableHttpService::builder()
//!         .service_factory(Arc::new(|| Ok(MyService::new())))
//!         .session_manager(Arc::new(LocalSessionManager::default()))
//!         .stateful_mode(true)
//!         .sse_keep_alive(Duration::from_secs(30))
//!         .build();
//!
//!     HttpServer::new(move || {
//!         App::new()
//!             // Mount StreamableHttp service at /api/v1/mcp/ (cloned for each worker)
//!             .service(web::scope("/api/v1/mcp").service(http_service.clone().scope()))
//!     })
//!     .bind("127.0.0.1:8080")?
//!     .run()
//!     .await
//! }
//! ```
//!
//! ## Propagating Extensions from Middleware
//!
//! Use the `on_request` hook to propagate typed data from actix-web middleware
//! to MCP request handlers. This is useful for passing authentication claims,
//! request metadata, or other context from HTTP middleware to your MCP service:
//!
//! ```rust,ignore
//! use rmcp_actix_web::transport::StreamableHttpService;
//! use actix_web::HttpMessage;
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct JwtClaims { user_id: String }
//!
//! let service = StreamableHttpService::builder()
//!     .service_factory(Arc::new(|| Ok(MyService::new())))
//!     .session_manager(Arc::new(LocalSessionManager::default()))
//!     .on_request_fn(|http_req, ext| {
//!         // Access data populated by actix-web middleware
//!         if let Some(claims) = http_req.extensions().get::<JwtClaims>() {
//!             ext.insert(claims.clone());
//!         }
//!     })
//!     .build();
//! ```
//!
//! The propagated extensions are then accessible in your MCP service handlers
//! via `RequestContext::extensions`.
//!
//! ## Protocol Compatibility
//!
//! The transport implements the [MCP protocol specification][mcp] and is compatible
//! with all MCP clients that support HTTP transports. The wire protocol is
//! identical to the Axum-based transports in the main [RMCP crate][rmcp].
//!
//! [mcp]: https://modelcontextprotocol.io/
//! [rmcp]: https://docs.rs/rmcp/

/// Streamable HTTP transport implementation.
///
/// Provides bidirectional communication with session management.
#[cfg(feature = "transport-streamable-http")]
pub mod streamable_http_server;
#[cfg(feature = "transport-streamable-http")]
pub use streamable_http_server::{
    OnRequestHook, StreamableHttpServerConfig, StreamableHttpService, StreamableHttpServiceBuilder,
};

/// Re-export of rmcp's Extensions type for use with on_request hook.
pub use rmcp::model::Extensions;

/// Authorization header value for MCP proxy scenarios.
///
/// This type is used to pass Authorization headers from HTTP requests
/// to MCP services via RequestContext extensions. This enables MCP services
/// to act as proxies, forwarding authentication tokens to backend APIs.
///
/// # Example
///
/// ```rust,ignore
/// // In an MCP service handler:
/// use rmcp_actix_web::transport::AuthorizationHeader;
///
/// async fn handle_request(
///     &self,
///     request: SomeRequest,
///     context: RequestContext<RoleServer>,
/// ) -> Result<Response, McpError> {
///     // Extract the Authorization header if present
///     if let Some(auth) = context.extensions.get::<AuthorizationHeader>() {
///         // Use auth.0 to access the header value (e.g., "Bearer token123")
///         let token = &auth.0;
///         // Forward to backend API...
///     }
///     // ...
/// }
/// ```
#[derive(Clone, Debug)]
pub struct AuthorizationHeader(pub String);
