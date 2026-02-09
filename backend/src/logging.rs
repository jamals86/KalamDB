// Logging module — powered by tracing-subscriber
//
// Uses tracing-subscriber for structured spans & events.
// A compatibility bridge (`tracing_log::LogTracer`) captures all existing
// `log::*` macro calls and routes them through the tracing subscriber so
// span context is preserved end-to-end.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::Path;

use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Log format type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Compact text format: timestamp LEVEL target - message
    Compact,
    /// JSON Lines format for structured logging
    Json,
}

impl LogFormat {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" | "jsonl" => LogFormat::Json,
            _ => LogFormat::Compact,
        }
    }
}

/// Build the `EnvFilter` from the base level, hardcoded noisy-crate
/// overrides, and optional per-target overrides from config.
fn build_env_filter(
    level: &str,
    target_levels: Option<&HashMap<String, String>>,
) -> anyhow::Result<EnvFilter> {
    // Base directive — set the default level
    let mut directives = vec![level.to_string()];

    // Suppress noisy third-party crates
    let noisy: &[(&str, &str)] = &[
        ("actix_server", "warn"),
        ("actix_web", "warn"),
        ("h2", "warn"),
        ("sqlparser", "warn"),
        ("datafusion", "warn"),
        ("datafusion_optimizer", "warn"),
        ("datafusion_datasource", "warn"),
        ("arrow", "warn"),
        ("parquet", "warn"),
        ("object_store", "info"),
        ("openraft", "error"),
        ("openraft::replication", "off"),
        ("tracing", "warn"),
    ];
    for (target, lvl) in noisy {
        directives.push(format!("{}={}", target, lvl));
    }

    // Per-target overrides from server.toml
    if let Some(map) = target_levels {
        for (target, lvl) in map.iter() {
            directives.push(format!("{}={}", target, lvl));
        }
    }

    let filter_str = directives.join(",");
    EnvFilter::try_new(&filter_str)
        .map_err(|e| anyhow::anyhow!("Invalid tracing filter '{}': {}", filter_str, e))
}

/// Initialize logging based on configuration.
///
/// Sets up `tracing-subscriber` with:
///  - Colored console layer (when `log_to_console` is true)
///  - File layer (compact text or JSON lines)
///  - `tracing_log::LogTracer` bridge so that all `log::*` calls are captured
///  - Span events on CLOSE (prints elapsed time for each span)
pub fn init_logging(
    level: &str,
    file_path: &str,
    log_to_console: bool,
    target_levels: Option<&HashMap<String, String>>,
    format: &str,
) -> anyhow::Result<()> {
    let log_format = LogFormat::from_str(format);

    // Create logs directory if it doesn't exist
    if let Some(parent) = Path::new(file_path).parent() {
        fs::create_dir_all(parent)?;
    }

    // Open log file in append mode
    let log_file = OpenOptions::new().create(true).append(true).open(file_path)?;

    // Bridge `log` crate → tracing (for all existing log::info!() etc. calls)
    tracing_log::LogTracer::init().ok(); // ok() in case already initialized

    // -- Console layer (optional) --
    let console_layer = if log_to_console {
        Some(
            tracing_subscriber::fmt::layer()
                .with_ansi(true)
                .with_target(true)
                .with_thread_names(true)
                .with_span_events(FmtSpan::CLOSE)
                .with_filter(build_env_filter(level, target_levels)?),
        )
    } else {
        None
    };

    // -- File layer --
    let file_layer = if log_format == LogFormat::Json {
        // JSON lines — includes span fields automatically
        let layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(log_file)
            .with_target(true)
            .with_thread_names(true)
            .with_span_events(FmtSpan::CLOSE)
            .with_span_list(true)
            .with_filter(build_env_filter(level, target_levels)?);
        // We need to box because the json() layer has a different type
        layer.boxed()
    } else {
        let layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(log_file)
            .with_target(true)
            .with_thread_names(true)
            .with_span_events(FmtSpan::CLOSE)
            .with_filter(build_env_filter(level, target_levels)?);
        layer.boxed()
    };

    // Compose and install as global subscriber
    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .init();

    tracing::trace!("Logging initialized: level={}, console={}, file={}", level, log_to_console, file_path);

    Ok(())
}

#[allow(dead_code)]
/// Initialize simple logging for development (console only)
pub fn init_simple_logging() -> anyhow::Result<()> {
    tracing_log::LogTracer::init().ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(true)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    Ok(())
}

#[cfg(test)]
mod tests {
    use kalamdb_commons::helpers::security::redact_sensitive_sql;

    #[test]
    fn test_redact_sensitive_sql_passwords() {
        // ALTER USER with SET PASSWORD
        let sql = "ALTER USER 'alice' SET PASSWORD 'SuperSecret123!'";
        let redacted = redact_sensitive_sql(sql);
        assert!(!redacted.contains("SuperSecret123"));
        assert!(redacted.contains("[REDACTED]"));

        // CREATE USER with PASSWORD
        let sql = "CREATE USER bob PASSWORD 'mypassword'";
        let redacted = redact_sensitive_sql(sql);
        assert!(!redacted.contains("mypassword"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_redact_sensitive_sql_preserves_safe_queries() {
        let sql = "SELECT * FROM users WHERE name = 'alice'";
        let redacted = redact_sensitive_sql(sql);
        assert_eq!(sql, redacted);
    }
}
