// KalamDB Server
//
// Main server binary for KalamDB - table-centric architecture with DataFusion

mod config;
mod logging;

use actix_web::{middleware, App, HttpServer};
use anyhow::Result;
use kalamdb_api::routes;
// use kalamdb_core::storage::RocksDbStore;
use log::info;

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

    // TODO: Initialize RocksDB, DataFusion, and other components
    info!("Server initialization - components will be added in Phase 2");

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting HTTP server on {}", bind_addr);
    info!("API endpoints will be implemented in upcoming phases");

    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
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
