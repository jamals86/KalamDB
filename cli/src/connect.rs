use crate::args::Cli;
use kalam_cli::{
    CLIConfiguration, CLIError, CLISession, FileCredentialStore, OutputFormat, Result,
};
use kalam_link::credentials::{CredentialStore, Credentials};
use kalam_link::{AuthProvider, KalamLinkClient, KalamLinkTimeouts, LoginResponse};
use std::time::Duration;

/// Build timeouts configuration from CLI arguments
fn build_timeouts(cli: &Cli) -> KalamLinkTimeouts {
    // Check for preset flags first
    if cli.fast_timeouts {
        return KalamLinkTimeouts::fast();
    }
    if cli.relaxed_timeouts {
        return KalamLinkTimeouts::relaxed();
    }

    // Build custom timeouts from individual CLI args
    KalamLinkTimeouts::builder()
        .connection_timeout_secs(cli.connection_timeout)
        .receive_timeout_secs(cli.receive_timeout)
        .send_timeout_secs(cli.timeout) // Use general timeout for send
        .auth_timeout_secs(cli.auth_timeout)
        .subscribe_timeout_secs(5) // Keep default for subscribe ack
        .initial_data_timeout_secs(cli.initial_data_timeout)
        .idle_timeout_secs(cli.subscription_timeout) // subscription_timeout is the idle timeout
        .keepalive_interval_secs(30) // Keep default
        .build()
}

pub async fn create_session(
    cli: &Cli,
    credential_store: &mut FileCredentialStore,
    config: &CLIConfiguration,
    config_path: std::path::PathBuf,
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
            if let Some(creds) = credential_store.get_credentials(&cli.instance).map_err(|e| {
                CLIError::ConfigurationError(format!("Failed to load credentials: {}", e))
            })? {
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
        },
    };

    // Helper function to check if URL is localhost
    fn is_localhost_url(url: &str) -> bool {
        url.contains("localhost")
            || url.contains("127.0.0.1")
            || url.contains("::1")
            || url.contains("0.0.0.0")
    }

    // Helper function to exchange username/password for JWT token
    async fn try_login(
        server_url: &str,
        username: &str,
        password: &str,
        verbose: bool,
    ) -> Option<LoginResponse> {
        // Create a temporary client just for login (no auth needed for login endpoint)
        let temp_client = match KalamLinkClient::builder()
            .base_url(server_url)
            .timeout(Duration::from_secs(10))
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                if verbose {
                    eprintln!("Warning: Could not create client for login: {}", e);
                }
                return None;
            },
        };

        match temp_client.login(username, password).await {
            Ok(response) => {
                if verbose {
                    eprintln!(
                        "Successfully authenticated as '{}' (expires: {})",
                        response.user.username, response.expires_at
                    );
                }
                Some(response)
            },
            Err(e) => {
                if verbose {
                    eprintln!("Warning: Login failed: {}", e);
                }
                None
            },
        }
    }

    // Determine authentication (priority: CLI args > stored credentials > localhost auto-auth)
    // Track: authenticated username, whether credentials were loaded from storage
    let (auth, authenticated_username, credentials_loaded) = if let Some(token) = cli
        .token
        .clone()
        .or_else(|| config.auth.as_ref().and_then(|a| a.jwt_token.clone()))
    {
        // Direct JWT token provided via --token or config - use it
        if cli.verbose {
            eprintln!("Using JWT token from CLI/config");
        }
        (AuthProvider::jwt_token(token), None, false)
    } else if let Some(username) = cli.username.clone() {
        // --username provided: login to get JWT token
        let password = cli.password.clone().unwrap_or_default();

        if let Some(login_response) =
            try_login(&server_url, &username, &password, cli.verbose).await
        {
            let authenticated_user = login_response.user.username.clone();

            // Only save credentials if --save-credentials flag is set
            if cli.save_credentials {
                let new_creds = Credentials::with_details(
                    cli.instance.clone(),
                    login_response.access_token.clone(),
                    login_response.user.username.clone(),
                    login_response.expires_at.clone(),
                    Some(server_url.clone()),
                );

                if let Err(e) = credential_store.set_credentials(&new_creds) {
                    if cli.verbose {
                        eprintln!("Warning: Could not save credentials: {}", e);
                    }
                } else if cli.verbose {
                    eprintln!("Saved JWT token for instance '{}'", cli.instance);
                }
            }

            if cli.verbose {
                eprintln!("Using JWT token for user '{}'", authenticated_user);
            }
            (
                AuthProvider::jwt_token(login_response.access_token),
                Some(authenticated_user),
                false,
            )
        } else {
            // Fallback to basic auth if login fails
            if cli.verbose {
                eprintln!("Login failed, falling back to basic auth for user '{}'", username);
            }
            (AuthProvider::basic_auth(username.clone(), password), Some(username), false)
        }
    } else if let Some(creds) = credential_store
        .get_credentials(&cli.instance)
        .map_err(|e| CLIError::ConfigurationError(format!("Failed to load credentials: {}", e)))?
    {
        // Load from stored credentials (JWT token)
        if creds.is_expired() {
            if cli.verbose {
                eprintln!("Stored JWT token for instance '{}' has expired", cli.instance);
            }
            // Token expired - need to re-authenticate with --username/--password
            return Err(CLIError::ConfigurationError(format!(
                "Stored credentials for '{}' have expired. Please login again with --username and --password --save-credentials",
                cli.instance
            )));
        }

        let stored_username = creds.username.clone();
        if cli.verbose {
            if let Some(ref user) = stored_username {
                eprintln!(
                    "Using stored JWT token for user '{}' (instance: {})",
                    user, cli.instance
                );
            } else {
                eprintln!("Using stored JWT token for instance '{}'", cli.instance);
            }
        }
        (AuthProvider::jwt_token(creds.jwt_token), stored_username, true)
    } else if is_localhost_url(&server_url) {
        // Auto-authenticate with root user for localhost connections
        let username = "root".to_string();
        let password = "".to_string();

        if let Some(login_response) =
            try_login(&server_url, &username, &password, cli.verbose).await
        {
            if cli.verbose {
                eprintln!("Auto-authenticated as root for localhost connection");
            }
            (
                AuthProvider::jwt_token(login_response.access_token),
                Some(login_response.user.username),
                false,
            )
        } else {
            if cli.verbose {
                eprintln!("Auto-login failed, using basic auth for localhost connection");
            }
            (AuthProvider::basic_auth(username.clone(), password), Some(username), false)
        }
    } else {
        (AuthProvider::None, None, false)
    };

    CLISession::with_auth_and_instance(
        server_url,
        auth,
        format,
        !cli.no_color,
        Some(cli.instance.clone()),
        Some(credential_store.clone()),
        authenticated_username,
        cli.loading_threshold_ms,
        !cli.no_spinner,
        Some(Duration::from_secs(cli.timeout)),
        Some(build_timeouts(cli)),
        Some(config.to_connection_options()),
        config.clone(),
        config_path,
        credentials_loaded,
    )
    .await
}
