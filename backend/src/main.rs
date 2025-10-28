// KalamDB Server entrypoint
//!
//! The heavy lifting (initialization, middleware wiring, graceful shutdown)
//! lives in dedicated modules so this file remains a thin orchestrator.

use kalamdb_server::{commands, config, middleware, routes};

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
    let args: Vec<String> = env::args().collect();

    // Check if create-user command is invoked
    if args.len() > 1 && args[1] == "create-user" {
        return handle_create_user(args).await;
    }

    // Normal server startup
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

    // Build application state and kick off background services
    let components = bootstrap(&config).await?;

    // Run HTTP server until termination signal is received
    run(&config, components).await
}

/// Handle create-user command
///
/// Usage: kalamdb-server create-user <username> <email> <role>
async fn handle_create_user(args: Vec<String>) -> Result<()> {
    if args.len() != 5 {
        eprintln!("Usage: kalamdb-server create-user <username> <email> <role>");
        eprintln!("Roles: admin, user, readonly");
        anyhow::bail!("Invalid arguments");
    }

    let username = &args[2];
    let email = &args[3];
    let role = &args[4];

    // Initialize minimal logging for command execution
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Creating user: {}", username);

    // Initialize RocksDB and SQL adapter
    let db_path = std::path::PathBuf::from("./data/rocksdb");
    std::fs::create_dir_all(&db_path)?;

    let db_init = kalamdb_core::storage::RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open()?;

    let kalam_sql = std::sync::Arc::new(kalamdb_sql::KalamSql::new(db)?);
    let sql_adapter = std::sync::Arc::new(kalam_sql.adapter().clone());

    // Create user
    let result = commands::create_user(sql_adapter, username, email, role).await?;

    println!("✅ User created successfully!");
    println!("Username: {}", username);
    println!("Email: {}", email);
    println!("Role: {}", role);
    println!("\n{}", result);

    Ok(())
}