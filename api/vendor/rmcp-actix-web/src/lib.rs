//! # rmcp-actix-web
//!
#![warn(missing_docs)]
//! actix-web transport implementations for RMCP (Rust Model Context Protocol).
//!
//! This crate provides HTTP-based transport layers for the [Model Context Protocol (MCP)][mcp],
//! offering a complete alternative to the default Axum-based transports in the main [RMCP crate][rmcp].
//! If you're already using actix-web in your application or prefer its API, this crate allows
//! you to integrate MCP services seamlessly without introducing additional web frameworks.
//!
//! [mcp]: https://modelcontextprotocol.io/
//! [rmcp]: https://crates.io/crates/rmcp
//!
//! ## Features
//!
//! - **[Streamable HTTP Transport][StreamableHttpService]**: Bidirectional communication with session management
//! - **Full MCP Compatibility**: Implements the complete MCP specification
//! - **Drop-in Replacement**: Same service implementations work with either Axum or actix-web transports
//! - **Production Ready**: Built on battle-tested actix-web framework
//!
//! ## Quick Start
//!
//! ### Streamable HTTP Server Example
//!
//! ```rust,no_run
//! use rmcp_actix_web::transport::{StreamableHttpService, AuthorizationHeader};
//! use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
//! use rmcp::{ServerHandler, model::ServerInfo};
//! use actix_web::{App, HttpServer};
//! use std::{sync::Arc, time::Duration};
//!
//! # #[derive(Clone)]
//! # struct MyMcpService;
//! # impl ServerHandler for MyMcpService {
//! #     fn get_info(&self) -> ServerInfo { ServerInfo::default() }
//! # }
//! # impl MyMcpService {
//! #     fn new() -> Self { Self }
//! # }
//! #[actix_web::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create service OUTSIDE HttpServer::new() to share across workers
//!     let http_service = StreamableHttpService::builder()
//!         .service_factory(Arc::new(|| Ok(MyMcpService::new())))
//!         .session_manager(Arc::new(LocalSessionManager::default()))
//!         .stateful_mode(true)
//!         .sse_keep_alive(Duration::from_secs(30))
//!         .build();
//!
//!     HttpServer::new(move || {
//!         App::new()
//!             // Clone service for each worker (shares the same LocalSessionManager)
//!             .service(http_service.clone().scope())
//!     })
//!     .bind("127.0.0.1:8080")?
//!     .run()
//!     .await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Examples
//!
//! See the `examples/` directory for complete working examples:
//! - `counter_streamable_http.rs` - Streamable HTTP server example
//! - `composition_streamable_http_example.rs` - StreamableHttp with custom mounting
//! - `authorization_proxy_example.rs` - Authorization header forwarding example
//!
//! ## Framework-Level Composition
//!
//! The transport supports framework-level composition aligned with RMCP patterns,
//! allowing you to mount MCP services at custom paths within existing actix-web applications.
//!
//! ### Service Composition
//!
//! ```rust,no_run
//! use rmcp_actix_web::transport::{StreamableHttpService, AuthorizationHeader};
//! use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
//! use actix_web::{App, web};
//! use std::{sync::Arc, time::Duration};
//!
//! # use rmcp::{ServerHandler, model::ServerInfo};
//! # #[derive(Clone)]
//! # struct MyService;
//! # impl ServerHandler for MyService {
//! #     fn get_info(&self) -> ServerInfo { ServerInfo::default() }
//! # }
//! # impl MyService { fn new() -> Self { Self } }
//! # use actix_web::HttpServer;
//! # #[actix_web::main]
//! # async fn main() -> std::io::Result<()> {
//! // Create service OUTSIDE HttpServer::new() to share across workers
//! let http_service = StreamableHttpService::builder()
//!     .service_factory(Arc::new(|| Ok(MyService::new())))
//!     .session_manager(Arc::new(LocalSessionManager::default()))
//!     .stateful_mode(true)
//!     .sse_keep_alive(Duration::from_secs(30))
//!     .build();
//!
//! HttpServer::new(move || {
//!     // Mount at custom path using scope() (cloned for each worker)
//!     App::new()
//!         .service(web::scope("/api/v1/calculator").service(http_service.clone().scope()))
//! }).bind("127.0.0.1:8080")?.run().await
//! # }
//! ```
//!
//
//! See the `examples/` directory for complete working examples of composition patterns.
//!
//! ## Performance Considerations
//!
//! - Streamable HTTP maintains persistent sessions which may use more memory
//! - Uses efficient async I/O through actix-web's actor system
//! - Framework-level composition adds minimal overhead
//!
//! ## Feature Flags
//!
//! - `transport-streamable-http` (default): Enables StreamableHttp transport

pub mod transport;
