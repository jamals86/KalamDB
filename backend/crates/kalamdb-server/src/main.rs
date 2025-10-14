// KalamDB Server
//
// Main server binary for KalamDB - SQL-only API

mod config;
mod logging;

use actix_web::{middleware, web, App, HttpServer};
use anyhow::Result;
use kalamdb_api::handlers::query::AppState;
use kalamdb_api::routes;
use kalamdb_core::{ids::SnowflakeGenerator, sql::SqlExecutor, storage::RocksDbStore};
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

    info!("Starting KalamDB Server v{} (SQL-only API)", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded: host={}, port={}", config.server.host, config.server.port);

    // Open RocksDB
    info!("Opening RocksDB at: {}", config.storage.rocksdb_path);
    let store = RocksDbStore::open_with_options(
        &config.storage.rocksdb_path,
        config.storage.enable_wal,
        &config.storage.compression,
    )?;
    info!("RocksDB opened successfully");

    // Create Snowflake ID generator (worker_id = 0 for single instance)
    let id_generator = Arc::new(SnowflakeGenerator::new(0));

    // Create SQL executor
    let sql_executor = Arc::new(SqlExecutor::new(
        Arc::new(store),
        id_generator,
        config.limits.max_message_size,
    ));

    // Create shared application state
    let app_state = web::Data::new(AppState { sql_executor });

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting HTTP server on {}", bind_addr);
    info!("API endpoint: POST /api/v1/query (SQL-only)");

    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
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
