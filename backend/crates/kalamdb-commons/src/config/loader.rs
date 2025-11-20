use super::types::ServerConfig;
use std::fs;
use std::path::Path;

impl ServerConfig {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;

        let mut config: ServerConfig = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;

        // Override with environment variables if present
        config.apply_env_overrides()?;

        config.validate()?;

        Ok(config)
    }

    /// Apply environment variable overrides for sensitive configuration
    ///
    /// Supported environment variables (T030):
    /// - KALAMDB_SERVER_HOST: Override server.host
    /// - KALAMDB_SERVER_PORT: Override server.port
    /// - KALAMDB_LOG_LEVEL: Override logging.level
    /// - KALAMDB_LOG_FILE: Override logging.file_path
    /// - KALAMDB_LOG_TO_CONSOLE: Override logging.log_to_console
    /// - KALAMDB_DATA_DIR: Override storage.rocksdb_path
    /// - KALAMDB_ROCKSDB_PATH: Override storage.rocksdb_path (legacy, prefer KALAMDB_DATA_DIR)
    /// - KALAMDB_LOG_FILE_PATH: Override logging.file_path (legacy, prefer KALAMDB_LOG_FILE)
    /// - KALAMDB_HOST: Override server.host (legacy, prefer KALAMDB_SERVER_HOST)
    /// - KALAMDB_PORT: Override server.port (legacy, prefer KALAMDB_SERVER_PORT)
    ///
    /// Environment variables take precedence over config.toml values (T031)
    fn apply_env_overrides(&mut self) -> anyhow::Result<()> {
        use std::env;

        // Server host (new naming convention)
        if let Ok(host) = env::var("KALAMDB_SERVER_HOST") {
            self.server.host = host;
        } else if let Ok(host) = env::var("KALAMDB_HOST") {
            // Legacy fallback
            self.server.host = host;
        }

        // Server port (new naming convention)
        if let Ok(port_str) = env::var("KALAMDB_SERVER_PORT") {
            self.server.port = port_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid KALAMDB_SERVER_PORT value: {}", port_str))?;
        } else if let Ok(port_str) = env::var("KALAMDB_PORT") {
            // Legacy fallback
            self.server.port = port_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid KALAMDB_PORT value: {}", port_str))?;
        }

        // Log level
        if let Ok(level) = env::var("KALAMDB_LOG_LEVEL") {
            self.logging.level = level;
        }

        // Logs directory path (new naming convention)
        if let Ok(path) = env::var("KALAMDB_LOGS_DIR") {
            self.logging.logs_path = path;
        } else if let Ok(path) = env::var("KALAMDB_LOG_FILE") {
            // Legacy fallback - extract directory from file path
            if let Some(parent) = std::path::Path::new(&path).parent() {
                self.logging.logs_path = parent.to_string_lossy().to_string();
            }
        } else if let Ok(path) = env::var("KALAMDB_LOG_FILE_PATH") {
            // Legacy fallback - extract directory from file path
            if let Some(parent) = std::path::Path::new(&path).parent() {
                self.logging.logs_path = parent.to_string_lossy().to_string();
            }
        }

        // Log to console
        if let Ok(val) = env::var("KALAMDB_LOG_TO_CONSOLE") {
            self.logging.log_to_console =
                val.to_lowercase() == "true" || val == "1" || val.to_lowercase() == "yes";
        }

        // Data directory (new naming convention)
        if let Ok(path) = env::var("KALAMDB_DATA_DIR") {
            self.storage.rocksdb_path = path;
        } else if let Ok(path) = env::var("KALAMDB_ROCKSDB_PATH") {
            // Legacy fallback
            self.storage.rocksdb_path = path;
        }

        Ok(())
    }

    /// Validate configuration settings
    pub fn validate(&self) -> anyhow::Result<()> {
        // Validate port range
        if self.server.port == 0 {
            return Err(anyhow::anyhow!("Server port cannot be 0"));
        }

        // Validate log level
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid log level '{}'. Must be one of: {}",
                self.logging.level,
                valid_levels.join(", ")
            ));
        }

        // Validate compression type
        let valid_compressions = ["none", "snappy", "zlib", "lz4", "zstd"];
        if !valid_compressions.contains(&self.storage.compression.as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid compression type '{}'. Must be one of: {}",
                self.storage.compression,
                valid_compressions.join(", ")
            ));
        }

        // Validate log format
        let valid_formats = ["compact", "pretty", "json"];
        if !valid_formats.contains(&self.logging.format.as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid log format '{}'. Must be one of: {}",
                self.logging.format,
                valid_formats.join(", ")
            ));
        }

        // Validate per-target log levels if provided
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        for (target, level) in &self.logging.targets {
            if !valid_levels.contains(&level.as_str()) {
                return Err(anyhow::anyhow!(
                    "Invalid log level '{}' for target '{}'. Must be one of: {}",
                    level,
                    target,
                    valid_levels.join(", ")
                ));
            }
        }

        // Validate message size limit
        if self.limits.max_message_size == 0 {
            return Err(anyhow::anyhow!("max_message_size cannot be 0"));
        }

        // Validate query limits
        if self.limits.max_query_limit == 0 {
            return Err(anyhow::anyhow!("max_query_limit cannot be 0"));
        }

        if self.limits.default_query_limit > self.limits.max_query_limit {
            return Err(anyhow::anyhow!(
                "default_query_limit ({}) cannot exceed max_query_limit ({})",
                self.limits.default_query_limit,
                self.limits.max_query_limit
            ));
        }

        if self.user_management.deletion_grace_period_days < 0 {
            return Err(anyhow::anyhow!(
                "deletion_grace_period_days cannot be negative"
            ));
        }

        if self.user_management.cleanup_job_schedule.trim().is_empty() {
            return Err(anyhow::anyhow!("cleanup_job_schedule cannot be empty"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn acquire_env_lock() -> MutexGuard<'static, ()> {
        ENV_MUTEX.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn test_default_config_is_valid() {
        let config = ServerConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_port() {
        let mut config = ServerConfig::default();
        config.server.port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_log_level() {
        let mut config = ServerConfig::default();
        config.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_compression() {
        let mut config = ServerConfig::default();
        config.storage.compression = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_env_override_server_host() {
        let _guard = acquire_env_lock();
        env::set_var("KALAMDB_SERVER_HOST", "0.0.0.0");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        env::remove_var("KALAMDB_SERVER_HOST");
    }

    #[test]
    fn test_env_override_server_port() {
        let _guard = acquire_env_lock();
        env::set_var("KALAMDB_SERVER_PORT", "9090");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.server.port, 9090);
        env::remove_var("KALAMDB_SERVER_PORT");
    }

    #[test]
    fn test_env_override_log_level() {
        let _guard = acquire_env_lock();
        env::set_var("KALAMDB_LOG_LEVEL", "debug");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.logging.level, "debug");
        env::remove_var("KALAMDB_LOG_LEVEL");
    }

    #[test]
    fn test_env_override_log_to_console() {
        let _guard = acquire_env_lock();
        env::set_var("KALAMDB_LOG_TO_CONSOLE", "false");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.logging.log_to_console, false);
        env::remove_var("KALAMDB_LOG_TO_CONSOLE");

        // Test truthy values
        env::set_var("KALAMDB_LOG_TO_CONSOLE", "true");
        config.apply_env_overrides().unwrap();
        assert_eq!(config.logging.log_to_console, true);
        env::remove_var("KALAMDB_LOG_TO_CONSOLE");

        env::set_var("KALAMDB_LOG_TO_CONSOLE", "1");
        config.apply_env_overrides().unwrap();
        assert_eq!(config.logging.log_to_console, true);
        env::remove_var("KALAMDB_LOG_TO_CONSOLE");
    }

    #[test]
    fn test_env_override_data_dir() {
        let _guard = acquire_env_lock();
        env::set_var("KALAMDB_DATA_DIR", "/custom/data");
        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.storage.rocksdb_path, "/custom/data");
        env::remove_var("KALAMDB_DATA_DIR");
    }

    #[test]
    fn test_legacy_env_vars() {
        // // Test legacy KALAMDB_HOST
        // env::set_var("KALAMDB_HOST", "192.168.1.1");
        // let mut config = ServerConfig::default();
        // config.apply_env_overrides().unwrap();
        // assert_eq!(config.server.host, "192.168.1.1");
        // env::remove_var("KALAMDB_HOST");

        // // Test legacy KALAMDB_PORT
        // env::set_var("KALAMDB_PORT", "3000");
        // config = ServerConfig::default();
        // config.apply_env_overrides().unwrap();
        // assert_eq!(config.server.port, 3000);
        // env::remove_var("KALAMDB_PORT");

        // // Test legacy KALAMDB_ROCKSDB_PATH
        // env::set_var("KALAMDB_ROCKSDB_PATH", "/old/path");
        // config = ServerConfig::default();
        // config.apply_env_overrides().unwrap();
        // assert_eq!(config.storage.rocksdb_path, "/old/path");
        // env::remove_var("KALAMDB_ROCKSDB_PATH");
    }

    #[test]
    fn test_new_env_vars_override_legacy() {
        let _guard = acquire_env_lock();
        // Set both new and legacy vars - new should win
        env::set_var("KALAMDB_SERVER_HOST", "new.host");
        env::set_var("KALAMDB_HOST", "old.host");

        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.server.host, "new.host");

        env::remove_var("KALAMDB_SERVER_HOST");
        env::remove_var("KALAMDB_HOST");

        // Same for port
        env::set_var("KALAMDB_SERVER_PORT", "8888");
        env::set_var("KALAMDB_PORT", "9999");

        config = ServerConfig::default();
        config.apply_env_overrides().unwrap();
        assert_eq!(config.server.port, 8888);

        env::remove_var("KALAMDB_SERVER_PORT");
        env::remove_var("KALAMDB_PORT");
    }
}
