// JWT configuration cache and trusted issuer parsing

use crate::providers::jwt_auth;
use once_cell::sync::OnceCell;

/// Cached JWT configuration for performance
///
/// Reading environment variables on every request is expensive.
/// This lazy static caches the configuration at first use.
pub struct JwtConfig {
    pub secret: String,
    pub trusted_issuers: Vec<String>,
}

static JWT_CONFIG: OnceCell<JwtConfig> = OnceCell::new();

/// Initialize JWT configuration from server settings.
///
/// This should be called once at startup after loading server.toml and applying
/// environment overrides. If not called, defaults are used.
pub fn init_jwt_config(secret: &str, trusted_issuers: &str) {
    let config = JwtConfig {
        secret: secret.to_string(),
        trusted_issuers: parse_trusted_issuers(trusted_issuers),
    };
    let _ = JWT_CONFIG.set(config);
}

pub fn get_jwt_config() -> &'static JwtConfig {
    JWT_CONFIG.get_or_init(|| JwtConfig {
        secret: kalamdb_configs::defaults::default_auth_jwt_secret(),
        trusted_issuers: parse_trusted_issuers(&kalamdb_configs::defaults::default_auth_jwt_trusted_issuers()),
    })
}

fn parse_trusted_issuers(input: &str) -> Vec<String> {
    let issuers: Vec<String> = input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if issuers.is_empty() {
        vec![jwt_auth::KALAMDB_ISSUER.to_string()]
    } else {
        issuers
    }
}
