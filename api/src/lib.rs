#[macro_use]
extern crate macro_attr;

#[macro_use]
extern crate enum_derive;

use std::{net::TcpListener, sync::Arc};

use actix_files as fs;
use actix_web::{dev::Server, http, middleware, web, App, HttpServer};
use anyhow::Context;
use configuration::Settings;
use core::time::Duration;
use tracing::info;
use tracing_actix_web::TracingLogger;

use crate::universal_inbox::{notification::service::NotificationService, UniversalInboxError};

pub mod commands;
pub mod configuration;
pub mod integrations;
pub mod observability;
pub mod repository;
pub mod routes;
pub mod universal_inbox;

pub async fn run(
    listener: TcpListener,
    settings: &Settings,
    service: Arc<NotificationService>,
) -> Result<Server, UniversalInboxError> {
    let api_path = settings.application.api_path.clone();
    let front_base_url = settings.application.front_base_url.clone();
    let static_path = settings.application.static_path.clone();
    let static_dir = settings
        .application
        .static_dir
        .clone()
        .unwrap_or_else(|| ".".to_string());
    let listen_address = listener.local_addr().unwrap();

    info!("Listening on {}", listen_address);

    let server = HttpServer::new(move || {
        info!(
            "Mounting API on {}",
            if api_path.is_empty() { "/" } else { &api_path }
        );
        let api_scope = web::scope(&api_path)
            .wrap(
                middleware::DefaultHeaders::new()
                    .add(("Access-Control-Allow-Origin", front_base_url.as_bytes()))
                    .add((
                        "Access-Control-Allow-Methods",
                        "POST, GET, OPTIONS, PATCH".as_bytes(),
                    ))
                    .add(("Access-Control-Allow-Headers", "content-type".as_bytes())),
            )
            .route("/notifications", web::get().to(routes::list_notifications))
            .route(
                "/notifications/sync",
                web::post().to(routes::sync_notifications),
            )
            .route(
                "/notifications/{notification_id}",
                web::get().to(routes::get_notification),
            )
            .route(
                "/notifications/{notification_id}",
                web::patch().to(routes::patch_notification),
            )
            .route(
                "/notifications/{notification_id}",
                web::method(http::Method::OPTIONS).to(routes::option_wildcard),
            )
            .route(
                "/notifications",
                web::post().to(routes::create_notification),
            )
            .route(
                "/notifications",
                web::method(http::Method::OPTIONS).to(routes::option_wildcard),
            )
            .app_data(web::Data::new(service.clone()));

        let mut app = App::new()
            .wrap(TracingLogger::default())
            .wrap(middleware::Compress::default())
            .route("/ping", web::get().to(routes::ping))
            .service(api_scope);
        if let Some(path) = &static_path {
            info!(
                "Mounting static files on {}",
                if path.is_empty() { "/" } else { path }
            );
            let static_scope = fs::Files::new(path, &static_dir)
                .use_last_modified(true)
                .index_file("index.html");
            app = app.service(static_scope);
        }
        app
    })
    .keep_alive(http::KeepAlive::Timeout(Duration::from_secs(60)))
    .shutdown_timeout(60)
    .listen(listener)
    .context(format!("Failed to listen on {}", listen_address))?;

    Ok(server.run())
}
