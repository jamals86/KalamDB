//! Configuration file management
//!
//! **Implements T085**: CLIConfiguration with TOML parsing for ~/.kalam/config.toml
//!
//! # Configuration Format
//!
//! ```toml
//! [server]
//! url = "http://localhost:8080"  # KalamDB server URL
//!
//! [auth]
//! jwt_token = "your-jwt-token"
//! # api_key = "your-api-key"  # Alternative to JWT
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT authentication token
    pub jwt_token: Option<String>,

    /// API key authentication
    pub api_key: Option<String>,
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
        let expanded_path = if path_str.starts_with("~/") {
            if let Some(home_dir) = dirs::home_dir() {
                home_dir.join(&path_str[2..])
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
        let expanded_path = if path_str.starts_with("~/") {
            if let Some(home_dir) = dirs::home_dir() {
                home_dir.join(&path_str[2..])
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CLIConfiguration::default();
        assert!(config.server.is_some());
        assert_eq!(
            config.server.unwrap().url,
            Some("http://localhost:8080".to_string())
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = CLIConfiguration::default();
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("[server]"));
        assert!(toml.contains("url"));
    }
}
