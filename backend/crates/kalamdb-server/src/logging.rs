// Logging module
use colored::*;
use log::{Level, LevelFilter};
use std::fs::{self, OpenOptions};
use std::io::Write;
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

/// Initialize logging based on configuration
/// Console pattern (colored): [timestamp] [LEVEL] - thread - module:line - message
/// File pattern (plain): [timestamp] [LEVEL] [thread - module:line] - message
pub fn init_logging(level: &str, file_path: &str, log_to_console: bool) -> anyhow::Result<()> {
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
        let base_config = fern::Dispatch::new()
            .level(level_filter);

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

        // File output without colors
        let file_config = fern::Dispatch::new()
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
            .chain(log_file);

        // Combine console and file outputs
        base_config
            .chain(console_config)
            .chain(file_config)
            .apply()?;

        log::info!("Logging initialized: level={}, console=yes, file={}", level, file_path);
    } else {
        // File only output without colors
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
            .chain(log_file)
            .apply()?;
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
    use env_logger::Builder;
    
    Builder::from_default_env()
        .filter_level(LevelFilter::Info)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .try_init()?;
    
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
}
