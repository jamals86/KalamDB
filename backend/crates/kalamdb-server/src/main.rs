// KalamDB Server entrypoint
//
// The heavy lifting (initialization, middleware wiring, graceful shutdown)
// lives in dedicated modules so this file remains a thin orchestrator.

mod config;
mod lifecycle;
mod logging;
mod middleware;
mod routes;

use anyhow::Result;
use config::ServerConfig;
use lifecycle::{bootstrap, run};
use log::info;

#[actix_web::main]
async fn main() -> Result<()> {
    // Load configuration (fallback to defaults when config file missing)
    let config = match ServerConfig::from_file("config.toml") {
        Ok(cfg) => cfg,
        Err(_) => {
            eprintln!("Warning: config.toml not found, using defaults");
            ServerConfig::default()
        }
    };

    // Logging before any other side effects
    logging::init_logging(
        &config.logging.level,
        &config.logging.file_path,
        config.logging.log_to_console,
    )?;

    info!(
        "Starting KalamDB Server v{}",
        env!("CARGO_PKG_VERSION")
    );
    info!(
        "Host: {}  Port: {}",
        config.server.host, config.server.port
    );

    // Build application state and kick off background services
    let components = bootstrap(&config).await?;

    // Run HTTP server until termination signal is received
    run(&config, components).await
}

