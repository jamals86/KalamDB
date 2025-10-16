// KalamDB Server
//
// Main server binary for KalamDB - table-centric architecture with DataFusion

mod config;
mod logging;

use actix_web::{middleware, web, App, HttpServer};
use actix_cors::Cors;
use anyhow::Result;
use kalamdb_api::routes;
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use log::info;
use std::sync::Arc;

#[actix_web::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = match config::ServerConfig::from_file("config.toml") {
        Ok(cfg) => cfg,
        Err(_) => {
            eprintln!("Warning: config.toml not found, using defaults");
            config::ServerConfig::default()
        }
    };

    // Initialize logging
    logging::init_logging(
        &config.logging.level,
        &config.logging.file_path,
        config.logging.log_to_console,
    )?;

    info!("Starting KalamDB Server v{} (Table-centric architecture)", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded: host={}, port={}", config.server.host, config.server.port);

    // Initialize DataFusion session factory
    let session_factory = Arc::new(
        DataFusionSessionFactory::new()
            .expect("Failed to create DataFusion session factory")
    );
    info!("DataFusion session factory initialized");

    // TODO: Initialize RocksDB and other components
    info!("Server initialization - additional components will be added in Phase 2");

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting HTTP server on {}", bind_addr);
    info!("Endpoints: POST /api/sql, GET /ws");

    // Start HTTP server
    HttpServer::new(move || {
        // Configure CORS for web browser clients
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .supports_credentials()
            .max_age(3600);
        
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(cors)
            .app_data(web::Data::new(session_factory.clone()))
            .configure(routes::configure_routes)
    })
    .bind(&bind_addr)?
    .workers(if config.server.workers == 0 {
        num_cpus::get()
    } else {
        config.server.workers
    })
    .run()
    .await?;

    info!("Server shutdown complete");
    Ok(())
}
