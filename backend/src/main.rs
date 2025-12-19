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

#[actix_web::main]
async fn main() -> Result<()> {
    // Normal server startup
    // Load configuration from server.toml (fallback to config.toml for backward compatibility)
    let config_path = if std::path::Path::new("server.toml").exists() {
        "server.toml"
    } else if std::path::Path::new("config.toml").exists() {
        eprintln!("⚠️  WARNING: Using deprecated config.toml. Please rename to server.toml");
        "config.toml"
    } else {
        eprintln!("❌ FATAL: Neither server.toml nor config.toml found");
        eprintln!("❌ Server cannot start without valid configuration");
        std::process::exit(1);
    };

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
            eprintln!("❌ FATAL: Failed to load {}: {}", config_path, e);
            eprintln!("❌ Server cannot start without valid configuration");
            std::process::exit(1);
        }
    };

    // ========================================================================
    // Security: Validate critical configuration at startup
    // ========================================================================
    
    // Check JWT secret strength
    const INSECURE_JWT_SECRETS: &[&str] = &[
        "CHANGE_ME_IN_PRODUCTION",
        "kalamdb-dev-secret-key-change-in-production",
        "your-secret-key-at-least-32-chars-change-me-in-production",
        "test",
        "secret",
        "password",
    ];
    
    let jwt_secret = &config.auth.jwt_secret;
    let is_insecure_secret = INSECURE_JWT_SECRETS.iter().any(|s| jwt_secret == *s);
    let is_short_secret = jwt_secret.len() < 32;
    
    if is_insecure_secret || is_short_secret {
        eprintln!("╔═══════════════════════════════════════════════════════════════════╗");
        eprintln!("║               ⚠️  SECURITY WARNING: JWT SECRET ⚠️                  ║");
        eprintln!("╠═══════════════════════════════════════════════════════════════════╣");
        if is_insecure_secret {
            eprintln!("║  The configured JWT secret is a known default/placeholder.       ║");
            eprintln!("║  This is INSECURE and allows token forgery!                       ║");
        }
        if is_short_secret {
            eprintln!("║  JWT secret is too short ({} chars). Minimum 32 chars required.  ║", jwt_secret.len());
        }
        eprintln!("║                                                                   ║");
        eprintln!("║  To fix: Set a strong, unique secret in server.toml:             ║");
        eprintln!("║    [auth]                                                         ║");
        eprintln!("║    jwt_secret = \"your-unique-32-char-minimum-secret-here\"         ║");
        eprintln!("║                                                                   ║");
        
        // In production mode (not localhost), refuse to start
        let host = &config.server.host;
        let is_localhost = host == "127.0.0.1" || host == "localhost" || host == "::1";
        
        if !is_localhost {
            eprintln!("║  FATAL: Refusing to start with insecure JWT secret on non-local  ║");
            eprintln!("║         address. This prevents token forgery attacks.             ║");
            eprintln!("╚═══════════════════════════════════════════════════════════════════╝");
            std::process::exit(1);
        } else {
            eprintln!("║  ⚠️ Allowing insecure secret for localhost development only.      ║");
            eprintln!("╚═══════════════════════════════════════════════════════════════════╝");
        }
    }

    // Logging before any other side effects
    // Use .jsonl extension for JSON format, .log for compact format
    let log_extension = if config.logging.format.eq_ignore_ascii_case("json") {
        "jsonl"
    } else {
        "log"
    };
    let server_log_path = format!("{}/server.{}", config.logging.logs_path, log_extension);
    logging::init_logging(
        &config.logging.level,
        &server_log_path,
        config.logging.log_to_console,
        Some(&config.logging.targets),
        &config.logging.format,
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
