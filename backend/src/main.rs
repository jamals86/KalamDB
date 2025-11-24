// KalamDB Server entrypoint
//!
//! The heavy lifting (initialization, middleware wiring, graceful shutdown)
//! lives in dedicated modules so this file remains a thin orchestrator.

use kalamdb_server::{config, middleware, routes};

mod lifecycle;
mod logging;

use anyhow::Result;
use config::ServerConfig;
use lifecycle::{bootstrap, run};
use log::info;
use std::env;

#[actix_web::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let _args: Vec<String> = env::args().collect();

    // Normal server startup
    // Load configuration (fallback to defaults when config file missing)
    let config_path = "config.toml";
    let config = match ServerConfig::from_file(config_path) {
        Ok(cfg) => {
            eprintln!(
                "✅ Loaded config from: {}",
                std::fs::canonicalize(config_path)
                    .unwrap_or_else(|_| std::path::PathBuf::from(config_path))
                    .display()
            );
            cfg
        }
        Err(e) => {
            eprintln!("❌ FATAL: Failed to load config.toml: {}", e);
            eprintln!("❌ Server cannot start without valid configuration");
            std::process::exit(1);
        }
    };

    // Logging before any other side effects
    let server_log_path = format!("{}/server.log", config.logging.logs_path);
    logging::init_logging(
        &config.logging.level,
        &server_log_path,
        config.logging.log_to_console,
        Some(&config.logging.targets),
    )?;

    // Display enhanced version information
    let version = env!("CARGO_PKG_VERSION");
    let commit = env!("GIT_COMMIT_HASH");
    let build_date = env!("BUILD_DATE");
    let branch = env!("GIT_BRANCH");

    info!("╔═══════════════════════════════════════════════════════════════╗");
    info!("║           KalamDB Server v{:<37} ║", version);
    info!("╠═══════════════════════════════════════════════════════════════╣");
    info!("║  Commit:     {:<49} ║", commit);
    info!("║  Branch:     {:<49} ║", branch);
    info!("║  Built:      {:<49} ║", build_date);
    info!("╚═══════════════════════════════════════════════════════════════╝");
    info!("Host: {}  Port: {}", config.server.host, config.server.port);

    // Check file descriptor limits (Unix only)
    #[cfg(unix)]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("sh").arg("-c").arg("ulimit -n").output() {
            let limit = String::from_utf8_lossy(&output.stdout);
            info!("File descriptor limit: {}", limit.trim());
            
            // Warn if limit is too low
            if let Ok(limit_num) = limit.trim().parse::<u32>() {
                if limit_num < 10000 {
                    log::warn!("╔═══════════════════════════════════════════════════════════════════╗");
                    log::warn!("║                    ⚠️  FILE DESCRIPTOR WARNING ⚠️                  ║");
                    log::warn!("╠═══════════════════════════════════════════════════════════════════╣");
                    log::warn!("║  Current limit: {:<52} ║", format!("{} file descriptors", limit_num));
                    log::warn!("║  Recommended:   {:<52} ║", "65536 file descriptors");
                    log::warn!("║                                                                   ║");
                    log::warn!("║  This limit may be too low for production use and could cause     ║");
                    log::warn!("║  'Too many open files' errors under heavy load.                  ║");
                    log::warn!("║                                                                   ║");
                    log::warn!("║  To increase the limit:                                           ║");
                    log::warn!("║    ulimit -n 65536                                                ║");
                    log::warn!("║                                                                   ║");
                    log::warn!("╚═══════════════════════════════════════════════════════════════════╝");
                }
            }
        }
    }

    // Build application state and kick off background services
    let (components, app_context) = bootstrap(&config).await?;

    // Run HTTP server until termination signal is received
    run(&config, components, app_context).await
}
