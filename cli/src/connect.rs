use crate::args::Cli;
use kalam_cli::{
    CLIConfiguration, CLIError, CLISession, FileCredentialStore, OutputFormat, Result,
};
use kalam_link::credentials::CredentialStore;
use kalam_link::AuthProvider;
use std::time::Duration;

pub async fn create_session(
    cli: &Cli,
    credential_store: &FileCredentialStore,
    config: &CLIConfiguration,
) -> Result<CLISession> {
    // Determine output format
    let format = if cli.json {
        OutputFormat::Json
    } else if cli.csv {
        OutputFormat::Csv
    } else {
        cli.format
    };

    // Determine server URL
    let server_url = match (cli.url.clone(), cli.host.clone()) {
        (Some(url), _) => url,
        (None, Some(host)) => format!("http://{}:{}", host, cli.port),
        (None, None) => {
            // Try to get from stored credentials first
            if let Some(creds) = credential_store
                .get_credentials(&cli.instance)
                .map_err(|e| {
                    CLIError::ConfigurationError(format!("Failed to load credentials: {}", e))
                })?
            {
                let creds_url = creds.get_server_url();
                // If credentials have a valid URL (starts with http), use it
                // Otherwise use default localhost:8080
                if creds_url.starts_with("http://") || creds_url.starts_with("https://") {
                    creds_url.to_string()
                } else {
                    // Default to localhost:8080
                    "http://localhost:8080".to_string()
                }
            } else {
                // Fallback to config file, or default to localhost:8080
                config
                    .server
                    .as_ref()
                    .and_then(|s| s.url.clone())
                    .unwrap_or_else(|| "http://localhost:8080".to_string())
            }
        }
    };

    // Helper function to check if URL is localhost
    fn is_localhost_url(url: &str) -> bool {
        url.contains("localhost")
            || url.contains("127.0.0.1")
            || url.contains("::1")
            || url.contains("0.0.0.0")
    }

    // Determine authentication (priority: CLI args > stored credentials > localhost auto-auth)
    let auth = if let Some(token) = cli
        .token
        .clone()
        .or_else(|| config.auth.as_ref().and_then(|a| a.jwt_token.clone()))
    {
        AuthProvider::jwt_token(token)
    } else if let (Some(username), Some(password)) = (cli.username.clone(), cli.password.clone()) {
        AuthProvider::basic_auth(username, password)
    } else if let Some(creds) = credential_store
        .get_credentials(&cli.instance)
        .map_err(|e| CLIError::ConfigurationError(format!("Failed to load credentials: {}", e)))?
    {
        // Load from stored credentials
        if cli.verbose {
            eprintln!("Using stored credentials for instance '{}'", cli.instance);
        }
        AuthProvider::basic_auth(creds.username, creds.password)
    } else if is_localhost_url(&server_url) {
        // Auto-authenticate with root user for localhost connections (no password needed from localhost)
        if cli.verbose {
            eprintln!("Auto-authenticating with root user for localhost connection");
        }
        AuthProvider::basic_auth("root".to_string(), String::new())
    } else {
        AuthProvider::None
    };

    CLISession::with_auth_and_instance(
        server_url,
        auth,
        format,
        !cli.no_color,
        Some(cli.instance.clone()),
        Some(credential_store.clone()),
        cli.loading_threshold_ms,
        !cli.no_spinner,
        Some(Duration::from_secs(cli.timeout)),
    )
    .await
}
