// KalamDB Server entrypoint
//!
//! The heavy lifting (initialization, middleware wiring, graceful shutdown)
//! lives in dedicated modules so this file remains a thin orchestrator.

use kalamdb_core::metrics::{BUILD_DATE, SERVER_VERSION};

mod logging;

use anyhow::Result;
use kalamdb_configs::ServerConfig;
use kalamdb_server::lifecycle::{bootstrap, run};
use log::info;

#[actix_web::main]
async fn main() -> Result<()> {
    let main_start = std::time::Instant::now();

    // Normal server startup
    // Use first CLI argument as config path; otherwise prefer server.toml in cwd, then next to binary
    let config_path = if let Some(arg_path) = std::env::args().nth(1) {
        std::path::PathBuf::from(arg_path)
    } else {
        let cwd_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("server.toml");
        if cwd_path.exists() {
            cwd_path
        } else {
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            exe_dir.join("server.toml")
        }
    };

    if !config_path.exists() {
        eprintln!("❌ FATAL: Config file not found: {}", config_path.display());
        eprintln!("❌ Server cannot start without valid configuration");
        std::process::exit(1);
    }

    let mut config = match ServerConfig::from_file(&config_path) {
        Ok(cfg) => {
            eprintln!(
                "✅ Loaded config from: {}",
                std::fs::canonicalize(&config_path)
                    .unwrap_or_else(|_| config_path.clone())
                    .display()
            );
            cfg
        },
        Err(e) => {
            eprintln!("❌ FATAL: Failed to load {}: {}", config_path.display(), e);
            eprintln!("❌ Server cannot start without valid configuration");
            std::process::exit(1);
        },
    };

    if let Err(e) = config.apply_env_overrides() {
        eprintln!("❌ FATAL: Failed to apply environment overrides: {}", e);
        eprintln!("❌ Server cannot start without valid configuration");
        std::process::exit(1);
    }

    if let Err(e) = config.finalize() {
        eprintln!("❌ FATAL: Invalid configuration after overrides: {}", e);
        eprintln!("❌ Server cannot start without valid configuration");
        std::process::exit(1);
    }

    // ========================================================================
    // JWT SECRET ENVIRONMENT VARIABLE SETUP
    // ========================================================================
    // IMPORTANT: Set KALAMDB_JWT_SECRET env var from config BEFORE any auth code runs
    // The JWT validation code (unified.rs) uses a lazy static that reads this env var
    // We must set it early so both login and validation use the SAME secret
    if std::env::var("KALAMDB_JWT_SECRET").is_err() {
        std::env::set_var("KALAMDB_JWT_SECRET", &config.auth.jwt_secret);
    }

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
            eprintln!(
                "║  JWT secret is too short ({} chars). Minimum 32 chars required.  ║",
                jwt_secret.len()
            );
        }
        eprintln!("║                                                                   ║");
        eprintln!("║  To fix: Set a strong, unique secret in server.toml:             ║");
        eprintln!("║    [auth]                                                         ║");
        eprintln!("║    jwt_secret = \"your-unique-32-char-minimum-secret-here\"         ║");
        eprintln!("║                                                                   ║");
        eprintln!("║  Or set via environment variable:                                ║");
        eprintln!("║    export KALAMDB_JWT_SECRET=\"$(openssl rand -base64 32)\"         ║");
        eprintln!("║                                                                   ║");
        eprintln!("║  Generate a secure random secret:                                ║");
        eprintln!("║    openssl rand -base64 32                                        ║");
        eprintln!("║    # or                                                           ║");
        eprintln!("║    cat /dev/urandom | head -c 32 | base64                        ║");
        eprintln!("║                                                                   ║");

        // In production mode (not localhost), refuse to start
        let host = &config.server.host;
        let is_localhost = host == "127.0.0.1" || host == "localhost" || host == "::1";

        if !is_localhost {
            eprintln!("║  FATAL: Refusing to start with insecure JWT secret on non-local  ║");
            eprintln!("║         address. This prevents token forgery attacks.             ║");
            eprintln!("║                                                                   ║");
            eprintln!("║  KalamDB will not start on {} with the current JWT secret.       ║", host);
            eprintln!("╚═══════════════════════════════════════════════════════════════════╝");
            std::process::exit(1);
        } else {
            eprintln!("║  ⚠️ Allowing insecure secret for localhost development only.      ║");
            eprintln!("║  This configuration would be REJECTED for production use.        ║");
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
    info!("KalamDB Server v{:<37}", SERVER_VERSION);
    info!("Build date: {}", BUILD_DATE);

    // Build application state and kick off background services
    let (components, app_context) = bootstrap(&config).await?;

    // Run HTTP server until termination signal is received
    run(&config, components, app_context, main_start).await
}
