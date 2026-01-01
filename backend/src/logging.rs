// Logging module
use colored::*;
use log::{Level, LevelFilter};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::Path;

/// Format log level with color for console
fn format_level_colored(level: Level) -> ColoredString {
    match level {
        Level::Error => format!("[{:5}]", level).bright_red().bold(),
        Level::Warn => format!("[{:5}]", level).bright_yellow().bold(),
        Level::Info => format!("[{:5}]", level).bright_green().bold(),
        Level::Debug => format!("[{:5}]", level).bright_blue().bold(),
        Level::Trace => format!("[{:5}]", level).bright_magenta().bold(),
    }
}

/// Log format type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Compact text format: [timestamp] [LEVEL] [thread - target:line] - message
    Compact,
    /// JSON Lines format for structured logging and DataFusion queries
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

/// Initialize logging based on configuration
/// Console pattern (colored): [timestamp] [LEVEL] - thread - module:line - message
/// File pattern (compact): [timestamp] [LEVEL] [thread - module:line] - message
/// File pattern (json): {"timestamp":"...","level":"...","thread":"...","target":"...","line":N,"message":"..."}
pub fn init_logging(
    level: &str,
    file_path: &str,
    log_to_console: bool,
    target_levels: Option<&HashMap<String, String>>,
    format: &str,
) -> anyhow::Result<()> {
    // Parse log format
    let log_format = LogFormat::from_str(format);
    // Parse log level
    let level_filter = parse_log_level(level)?;

    // Create logs directory if it doesn't exist
    if let Some(parent) = Path::new(file_path).parent() {
        fs::create_dir_all(parent)?;
    }

    // Open log file in append mode
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?;

    if log_to_console {
        // Setup dual logging: colored console + plain file
        let mut base_config = fern::Dispatch::new()
            .level(level_filter)
            // Filter out noisy third-party debug logs
            //actix_server
            .level_for("actix_server", LevelFilter::Warn)
            .level_for("actix_web", LevelFilter::Warn)
            .level_for("h2", LevelFilter::Warn)
            .level_for("sqlparser", LevelFilter::Warn)
            .level_for("datafusion", LevelFilter::Warn)
            .level_for("datafusion_optimizer", LevelFilter::Warn)
            .level_for("datafusion_datasource", LevelFilter::Warn)
            .level_for("arrow", LevelFilter::Warn)
            .level_for("parquet", LevelFilter::Warn)
            .level_for("object_store", LevelFilter::Info);

        // Apply per-target overrides from configuration (if any)
        if let Some(map) = target_levels {
            for (target, lvl) in map.iter() {
                if let Ok(parsed) = parse_log_level(lvl) {
                    // fern::level_for expects a 'static str; leak the config string (tiny, one-time)
                    let target_static: &'static str = Box::leak(target.clone().into_boxed_str());
                    base_config = base_config.level_for(target_static, parsed);
                } else {
                    eprintln!(
                        "Ignoring invalid log level '{}' for target '{}' in config",
                        lvl, target
                    );
                }
            }
        }

        // Console output with colors
        let console_config = fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "{} {} - {} - {}",
                    format!("[{}]", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"))
                        .bright_green()
                        .bold(),
                    format_level_colored(record.level()),
                    format!(
                        "{} - {}:{}",
                        std::thread::current().name().unwrap_or("main"),
                        record.target(),
                        record.line().unwrap_or(0)
                    )
                    .bright_magenta(),
                    message
                ))
            })
            .chain(std::io::stdout());

        // File output - format depends on config
        let file_config = if log_format == LogFormat::Json {
            // JSON Lines format for structured logging
            fern::Dispatch::new()
                .format(|out, message, record| {
                    let json = serde_json::json!({
                        "timestamp": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
                        "level": record.level().to_string(),
                        "thread": std::thread::current().name().unwrap_or("main"),
                        "target": record.target(),
                        "line": record.line().unwrap_or(0),
                        "message": message.to_string()
                    });
                    out.finish(format_args!("{}", json))
                })
                .chain(log_file)
        } else {
            // Compact text format
            fern::Dispatch::new()
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "[{}] [{:5}] [{} - {}:{}] - {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                        record.level(),
                        std::thread::current().name().unwrap_or("main"),
                        record.target(),
                        record.line().unwrap_or(0),
                        message
                    ))
                })
                .chain(log_file)
        };

        // Combine console and file outputs
        base_config
            .chain(console_config)
            .chain(file_config)
            .apply()?;

        log::trace!(
            "Logging initialized: level={}, console=yes, file={}",
            level,
            file_path
        );
    } else {
        // File only output - use JSON or compact format based on config
        let mut file_only = if log_format == LogFormat::Json {
            fern::Dispatch::new()
                .level(level_filter)
                .format(|out, message, record| {
                    let json = serde_json::json!({
                        "timestamp": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
                        "level": record.level().to_string(),
                        "thread": std::thread::current().name().unwrap_or("main"),
                        "target": record.target(),
                        "line": record.line().unwrap_or(0),
                        "message": message.to_string()
                    });
                    out.finish(format_args!("{}", json))
                })
        } else {
            fern::Dispatch::new()
                .level(level_filter)
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "[{}] [{:5}] [{} - {}:{}] - {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                        record.level(),
                        std::thread::current().name().unwrap_or("main"),
                        record.target(),
                        record.line().unwrap_or(0),
                        message
                    ))
                })
        };

        // Apply per-target overrides from configuration (if any)
        if let Some(map) = target_levels {
            for (target, lvl) in map.iter() {
                if let Ok(parsed) = parse_log_level(lvl) {
                    let target_static: &'static str = Box::leak(target.clone().into_boxed_str());
                    file_only = file_only.level_for(target_static, parsed);
                } else {
                    eprintln!(
                        "Ignoring invalid log level '{}' for target '{}' in config",
                        lvl, target
                    );
                }
            }
        }

        file_only.chain(log_file).apply()?;
    }

    Ok(())
}

/// Parse log level string to LevelFilter
fn parse_log_level(level: &str) -> anyhow::Result<LevelFilter> {
    match level.to_lowercase().as_str() {
        "error" => Ok(LevelFilter::Error),
        "warn" => Ok(LevelFilter::Warn),
        "info" => Ok(LevelFilter::Info),
        "debug" => Ok(LevelFilter::Debug),
        "trace" => Ok(LevelFilter::Trace),
        _ => Err(anyhow::anyhow!("Invalid log level: {}", level)),
    }
}

#[allow(dead_code)]
/// Initialize simple logging for development (console only)
pub fn init_simple_logging() -> anyhow::Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .level(LevelFilter::Info)
        .chain(std::io::stdout())
        .apply()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_level() {
        assert!(matches!(parse_log_level("error"), Ok(LevelFilter::Error)));
        assert!(matches!(parse_log_level("warn"), Ok(LevelFilter::Warn)));
        assert!(matches!(parse_log_level("info"), Ok(LevelFilter::Info)));
        assert!(matches!(parse_log_level("debug"), Ok(LevelFilter::Debug)));
        assert!(matches!(parse_log_level("trace"), Ok(LevelFilter::Trace)));
        assert!(parse_log_level("invalid").is_err());
    }

    #[test]
    fn test_parse_log_level_case_insensitive() {
        assert!(matches!(parse_log_level("INFO"), Ok(LevelFilter::Info)));
        assert!(matches!(parse_log_level("Debug"), Ok(LevelFilter::Debug)));
    }

    #[test]
    fn test_redact_sensitive_sql_passwords() {
        use kalamdb_commons::security::redact_sensitive_sql;
        
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
        use kalamdb_commons::security::redact_sensitive_sql;
        
        let sql = "SELECT * FROM users WHERE name = 'alice'";
        let redacted = redact_sensitive_sql(sql);
        assert_eq!(sql, redacted);
    }
}
