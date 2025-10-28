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
            .level(level_filter)
            // Filter out noisy third-party debug logs
            .level_for("sqlparser", LevelFilter::Debug)
            .level_for("datafusion", LevelFilter::Debug)
            .level_for("arrow", LevelFilter::Debug)
            .level_for("parquet", LevelFilter::Debug)
            .level_for("object_store", LevelFilter::Debug);

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

        log::trace!(
            "Logging initialized: level={}, console=yes, file={}",
            level,
            file_path
        );
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
    
    #[test]
    fn test_redact_password_key_value() {
        let message = "Creating user with password=secret123 and email=test@example.com";
        let redacted = redact_sensitive_data(message);
        assert!(!redacted.contains("secret123"), "Password not redacted: {}", redacted);
        assert!(redacted.contains("[REDACTED]"), "REDACTED marker missing: {}", redacted);
        assert!(redacted.contains("email=test@example.com"), "Non-sensitive data removed: {}", redacted);
    }
    
    #[test]
    fn test_redact_password_json() {
        let message = r#"User data: {"username": "alice", "password": "secret123"}"#;
        let redacted = redact_sensitive_data(message);
        assert!(!redacted.contains("secret123"), "Password not redacted: {}", redacted);
        assert!(redacted.contains("[REDACTED]"), "REDACTED marker missing: {}", redacted);
    }
    
    #[test]
    fn test_redact_auth_token() {
        let message = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let redacted = redact_sensitive_data(message);
        assert!(!redacted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"), "Token not redacted: {}", redacted);
        assert!(redacted.contains("[REDACTED]"), "REDACTED marker missing: {}", redacted);
    }
    
    #[test]
    fn test_redact_multiple_sensitive_fields() {
        let message = "password=pass123 api_key=key456 email=test@example.com";
        let redacted = redact_sensitive_data(message);
        assert!(!redacted.contains("pass123"), "Password not redacted");
        assert!(!redacted.contains("key456"), "API key not redacted");
        assert!(redacted.contains("email=test@example.com"), "Email should not be redacted");
    }
    
    #[test]
    fn test_is_sensitive_field() {
        assert!(is_sensitive_field("password"));
        assert!(is_sensitive_field("PASSWORD"));
        assert!(is_sensitive_field("user_password"));
        assert!(is_sensitive_field("api_key"));
        assert!(is_sensitive_field("secret"));
        assert!(is_sensitive_field("auth_data"));
        
        assert!(!is_sensitive_field("username"));
        assert!(!is_sensitive_field("email"));
        assert!(!is_sensitive_field("user_id"));
    }
}

// =============================================================================
// Password Redaction (T126 - FR-SEC-001)
// =============================================================================

/// Sensitive fields that should be redacted from logs
const SENSITIVE_FIELDS: &[&str] = &[
    "password",
    "pass",
    "pwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "auth_data",
    "credentials",
    "authorization",
];

/// Check if a field name is sensitive and should be redacted
fn is_sensitive_field(field_name: &str) -> bool {
    let lower_field = field_name.to_lowercase();
    SENSITIVE_FIELDS.iter().any(|&sensitive| {
        lower_field.contains(sensitive)
    })
}

/// Redact sensitive information from log message
///
/// Replaces password and other sensitive fields with "[REDACTED]"
///
/// Examples:
/// - "password=secret123" → "password=[REDACTED]"
/// - "Creating user with password: foo" → "Creating user with password: [REDACTED]"
/// - "auth_data={'secret': 'bar'}" → "auth_data=[REDACTED]"
///
/// # Arguments
/// * `message` - Original log message
///
/// # Returns
/// Redacted message safe for logging
pub fn redact_sensitive_data(message: &str) -> String {
    let mut redacted = message.to_string();
    
    // Pattern 1: key=value (e.g., "password=secret123")
    for field in SENSITIVE_FIELDS {
        let patterns = vec![
            // password=secret123
            format!(r"{}=\S+", field),
            // password = secret123
            format!(r"{}\s*=\s*\S+", field),
            // password: secret123
            format!(r"{}:\s*\S+", field),
            // "password": "secret123"
            format!(r#""{}":\s*"[^"]*""#, field),
            // 'password': 'secret123'
            format!(r"'{}':\s*'[^']*'", field),
        ];
        
        for pattern in patterns {
            if let Ok(re) = regex::Regex::new(&pattern) {
                redacted = re.replace_all(&redacted, |caps: &regex::Captures| {
                    let full_match = caps.get(0).unwrap().as_str();
                    let separator_idx = full_match.find(|c| c == '=' || c == ':').unwrap();
                    format!("{}[REDACTED]", &full_match[..=separator_idx])
                }).to_string();
            }
        }
    }
    
    // Pattern 2: field: value in logs (e.g., "with password: secret123")
    for field in SENSITIVE_FIELDS {
        let pattern = format!(r"{}\s*:\s*\S+", field);
        if let Ok(re) = regex::Regex::new(&pattern) {
            redacted = re.replace_all(&redacted, |_: &regex::Captures| {
                format!("{}: [REDACTED]", field)
            }).to_string();
        }
    }
    
    redacted
}

// =============================================================================
// Authentication Logging
// =============================================================================

/// Log an authentication failure
///
/// Logs to both the main log and auth.log for security auditing.
///
/// # Arguments
/// * `username` - Username that attempted to authenticate
/// * `source_ip` - IP address of the client
/// * `failure_reason` - Reason for authentication failure
/// * `request_id` - Optional request ID for correlation
pub fn log_auth_failure(
    username: &str,
    source_ip: &str,
    failure_reason: &str,
    request_id: Option<&str>,
) {
    let req_id = request_id.unwrap_or("N/A");
    log::warn!(
        target: "kalamdb::auth",
        "[AUTH_FAILURE] username={} source_ip={} reason={} request_id={}",
        username,
        source_ip,
        failure_reason,
        req_id
    );
}

/// Log a successful authentication
///
/// # Arguments
/// * `username` - Username that authenticated
/// * `source_ip` - IP address of the client
/// * `auth_method` - Authentication method used (e.g., "password", "jwt", "apikey")
/// * `request_id` - Optional request ID for correlation
pub fn log_auth_success(
    username: &str,
    source_ip: &str,
    auth_method: &str,
    request_id: Option<&str>,
) {
    let req_id = request_id.unwrap_or("N/A");
    log::info!(
        target: "kalamdb::auth",
        "[AUTH_SUCCESS] username={} source_ip={} method={} request_id={}",
        username,
        source_ip,
        auth_method,
        req_id
    );
}

/// Log a user role change
///
/// # Arguments
/// * `target_user_id` - User whose role was changed
/// * `old_role` - Previous role
/// * `new_role` - New role
/// * `admin_user_id` - Admin who performed the change
pub fn log_role_change(
    target_user_id: &str,
    old_role: &str,
    new_role: &str,
    admin_user_id: &str,
) {
    log::warn!(
        target: "kalamdb::auth",
        "[ROLE_CHANGE] target_user={} old_role={} new_role={} admin_user={}",
        target_user_id,
        old_role,
        new_role,
        admin_user_id
    );
}

/// Log an administrative operation
///
/// # Arguments
/// * `admin_user_id` - Admin performing the operation
/// * `operation` - Operation being performed (e.g., "create_user", "delete_table")
/// * `target_user_id` - Optional target user ID
/// * `result` - Result of the operation ("success" or error message)
pub fn log_admin_operation(
    admin_user_id: &str,
    operation: &str,
    target_user_id: Option<&str>,
    result: &str,
) {
    let target = target_user_id.unwrap_or("N/A");
    log::info!(
        target: "kalamdb::auth",
        "[ADMIN_OP] admin_user={} operation={} target_user={} result={}",
        admin_user_id,
        operation,
        target,
        result
    );
}

/// Log a permission check
///
/// # Arguments
/// * `user_id` - User requesting access
/// * `resource_type` - Type of resource (e.g., "table", "namespace")
/// * `resource_id` - Resource identifier
/// * `permission` - Permission being checked (e.g., "read", "write", "delete")
/// * `granted` - Whether permission was granted
pub fn log_permission_check(
    user_id: &str,
    resource_type: &str,
    resource_id: &str,
    permission: &str,
    granted: bool,
) {
    let level = if granted {
        log::Level::Debug
    } else {
        log::Level::Warn
    };

    log::log!(
        target: "kalamdb::auth",
        level,
        "[PERMISSION_CHECK] user={} resource={}:{} permission={} granted={}",
        user_id,
        resource_type,
        resource_id,
        permission,
        granted
    );
}
