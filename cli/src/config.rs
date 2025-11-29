//! Configuration file management
//!
//! **Implements T085**: CLIConfiguration with TOML parsing for ~/.kalam/config.toml
//!
//! # Configuration Format
//!
//! ```toml
//! [server]
//! url = "http://localhost:8080"  # KalamDB server URL
//! http_version = "http2"         # HTTP version: "http1", "http2", "auto"
//!
//! [connection]
//! auto_reconnect = true          # Auto-reconnect on connection loss
//! reconnect_delay_ms = 100       # Initial reconnect delay
//! max_reconnect_delay_ms = 30000 # Maximum reconnect delay
//! max_reconnect_attempts = 10    # Max reconnect attempts (0 = unlimited)
//!
//! [auth]
//! jwt_token = "your-jwt-token"
//!
//! [ui]
//! format = "table"  # table, json, csv
//! color = true
//! history_size = 1000
//! ```

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{CLIError, Result};

/// CLI configuration loaded from TOML file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CLIConfiguration {
    /// Server connection settings
    pub server: Option<ServerConfig>,

    /// Connection/reconnection settings
    pub connection: Option<ConnectionConfig>,

    /// Authentication settings
    pub auth: Option<AuthConfig>,

    /// UI preferences
    pub ui: Option<UIConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server URL (e.g., http://localhost:3000)
    pub url: Option<String>,

    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Maximum retry attempts
    #[serde(default = "default_retries")]
    pub max_retries: u32,

    /// HTTP version preference: "http1", "http2", "auto" (default: "http2")
    #[serde(default = "default_http_version")]
    pub http_version: String,
}

/// Connection settings for reconnection behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Enable automatic reconnection on connection loss (default: true)
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,

    /// Initial delay between reconnection attempts in milliseconds (default: 100)
    #[serde(default = "default_reconnect_delay_ms")]
    pub reconnect_delay_ms: u64,

    /// Maximum delay between reconnection attempts in milliseconds (default: 30000)
    #[serde(default = "default_max_reconnect_delay_ms")]
    pub max_reconnect_delay_ms: u64,

    /// Maximum number of reconnection attempts (0 = unlimited, default: 10)
    #[serde(default = "default_max_reconnect_attempts")]
    pub max_reconnect_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT authentication token
    pub jwt_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfig {
    /// Output format: table, json, csv
    #[serde(default = "default_format")]
    pub format: String,

    /// Enable colored output
    #[serde(default = "default_color")]
    pub color: bool,

    /// Maximum history size
    #[serde(default = "default_history_size")]
    pub history_size: usize,
}

fn default_timeout() -> u64 {
    30
}

fn default_retries() -> u32 {
    3
}

fn default_http_version() -> String {
    "http2".to_string()
}

fn default_auto_reconnect() -> bool {
    true
}

fn default_reconnect_delay_ms() -> u64 {
    100
}

fn default_max_reconnect_delay_ms() -> u64 {
    30000
}

fn default_max_reconnect_attempts() -> u32 {
    10
}

fn default_format() -> String {
    "table".to_string()
}

fn default_color() -> bool {
    true
}

fn default_history_size() -> usize {
    1000
}

impl Default for CLIConfiguration {
    fn default() -> Self {
        Self {
            server: Some(ServerConfig {
                url: Some("http://localhost:8080".to_string()),
                timeout: default_timeout(),
                max_retries: default_retries(),
                http_version: default_http_version(),
            }),
            connection: Some(ConnectionConfig {
                auto_reconnect: default_auto_reconnect(),
                reconnect_delay_ms: default_reconnect_delay_ms(),
                max_reconnect_delay_ms: default_max_reconnect_delay_ms(),
                max_reconnect_attempts: default_max_reconnect_attempts(),
            }),
            auth: None,
            ui: Some(UIConfig {
                format: default_format(),
                color: default_color(),
                history_size: default_history_size(),
            }),
        }
    }
}

impl CLIConfiguration {
    /// Load configuration from file
    ///
    /// Returns default configuration if file doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        // Expand tilde manually using home directory
        let path_str = path.to_str().unwrap_or("~/.kalam/config.toml");
        let expanded_path = if let Some(rest) = path_str.strip_prefix("~/") {
            if let Some(home_dir) = dirs::home_dir() {
                home_dir.join(rest)
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };
        let path = &expanded_path;

        if !path.exists() {
            // Return default configuration if file doesn't exist
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path).map_err(|e| {
            CLIError::ConfigurationError(format!("Failed to read config file: {}", e))
        })?;

        let config: CLIConfiguration = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self, path: &Path) -> Result<()> {
        // Expand tilde manually using home directory
        let path_str = path.to_str().unwrap_or("~/.kalam/config.toml");
        let expanded_path = if let Some(rest) = path_str.strip_prefix("~/") {
            if let Some(home_dir) = dirs::home_dir() {
                home_dir.join(rest)
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };
        let path = &expanded_path;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)
            .map_err(|e| CLIError::ConfigurationError(format!("Failed to serialize: {}", e)))?;

        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Build ConnectionOptions from CLI configuration
    ///
    /// Converts CLI config settings to kalam-link ConnectionOptions
    pub fn to_connection_options(&self) -> kalam_link::ConnectionOptions {
        use kalam_link::{ConnectionOptions, HttpVersion};

        let mut options = ConnectionOptions::default();

        // Apply server settings
        if let Some(ref server) = self.server {
            // Parse http_version string to HttpVersion enum
            options = options.with_http_version(match server.http_version.to_lowercase().as_str() {
                "http1" | "http/1" | "http/1.1" => HttpVersion::Http1,
                "http2" | "http/2" => HttpVersion::Http2,
                "auto" | _ => HttpVersion::Auto,
            });
        }

        // Apply connection settings
        if let Some(ref conn) = self.connection {
            options = options
                .with_auto_reconnect(conn.auto_reconnect)
                .with_reconnect_delay_ms(conn.reconnect_delay_ms)
                .with_max_reconnect_delay_ms(conn.max_reconnect_delay_ms);

            // Convert 0 to None (unlimited), otherwise Some(n)
            let max_attempts = if conn.max_reconnect_attempts == 0 {
                None
            } else {
                Some(conn.max_reconnect_attempts)
            };
            options = options.with_max_reconnect_attempts(max_attempts);
        }

        options
    }

    /// Get the HTTP version setting from config
    pub fn http_version(&self) -> kalam_link::HttpVersion {
        use kalam_link::HttpVersion;

        self.server
            .as_ref()
            .map(|s| match s.http_version.to_lowercase().as_str() {
                "http1" | "http/1" | "http/1.1" => HttpVersion::Http1,
                "http2" | "http/2" => HttpVersion::Http2,
                "auto" | _ => HttpVersion::Auto,
            })
            .unwrap_or(HttpVersion::Http2) // Default to HTTP/2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CLIConfiguration::default();
        assert!(config.server.is_some());
        assert_eq!(
            config.server.as_ref().unwrap().url,
            Some("http://localhost:8080".to_string())
        );
        // HTTP/2 should be the default
        assert_eq!(config.server.as_ref().unwrap().http_version, "http2");
    }

    #[test]
    fn test_default_connection_config() {
        let config = CLIConfiguration::default();
        assert!(config.connection.is_some());
        let conn = config.connection.as_ref().unwrap();
        assert!(conn.auto_reconnect);
        assert_eq!(conn.reconnect_delay_ms, 100);
        assert_eq!(conn.max_reconnect_delay_ms, 30000);
        assert_eq!(conn.max_reconnect_attempts, 10);
    }

    #[test]
    fn test_config_serialization() {
        let config = CLIConfiguration::default();
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("[server]"));
        assert!(toml.contains("url"));
        assert!(toml.contains("http_version"));
        assert!(toml.contains("[connection]"));
        assert!(toml.contains("auto_reconnect"));
    }

    #[test]
    fn test_to_connection_options() {
        let config = CLIConfiguration::default();
        let options = config.to_connection_options();

        // Verify defaults are applied
        assert_eq!(options.http_version, kalam_link::HttpVersion::Http2);
        assert!(options.auto_reconnect);
        assert_eq!(options.reconnect_delay_ms, 100);
        assert_eq!(options.max_reconnect_delay_ms, 30000);
        assert_eq!(options.max_reconnect_attempts, Some(10));
    }

    #[test]
    fn test_http_version_parsing() {
        // Test various http_version strings
        let mut config = CLIConfiguration::default();

        // http1 variants
        config.server.as_mut().unwrap().http_version = "http1".to_string();
        assert_eq!(config.http_version(), kalam_link::HttpVersion::Http1);

        config.server.as_mut().unwrap().http_version = "http/1".to_string();
        assert_eq!(config.http_version(), kalam_link::HttpVersion::Http1);

        config.server.as_mut().unwrap().http_version = "HTTP/1.1".to_string();
        assert_eq!(config.http_version(), kalam_link::HttpVersion::Http1);

        // http2 variants
        config.server.as_mut().unwrap().http_version = "http2".to_string();
        assert_eq!(config.http_version(), kalam_link::HttpVersion::Http2);

        config.server.as_mut().unwrap().http_version = "HTTP/2".to_string();
        assert_eq!(config.http_version(), kalam_link::HttpVersion::Http2);

        // auto
        config.server.as_mut().unwrap().http_version = "auto".to_string();
        assert_eq!(config.http_version(), kalam_link::HttpVersion::Auto);

        // unknown defaults to Auto
        config.server.as_mut().unwrap().http_version = "unknown".to_string();
        assert_eq!(config.http_version(), kalam_link::HttpVersion::Auto);
    }

    #[test]
    fn test_unlimited_reconnect_attempts() {
        let mut config = CLIConfiguration::default();
        config.connection.as_mut().unwrap().max_reconnect_attempts = 0;

        let options = config.to_connection_options();
        // 0 should be converted to None (unlimited)
        assert_eq!(options.max_reconnect_attempts, None);
    }
}
