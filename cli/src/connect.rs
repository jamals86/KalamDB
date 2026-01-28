use crate::args::Cli;
use kalam_cli::{
    CLIConfiguration, CLIError, CLISession, FileCredentialStore, OutputFormat, Result,
};
use kalam_link::credentials::{CredentialStore, Credentials};
use kalam_link::{
    AuthProvider, KalamLinkClient, KalamLinkError, KalamLinkTimeouts, LoginResponse,
    ServerSetupRequest,
};
use std::io::{self, IsTerminal, Write};
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
                // Default to localhost:8080 (credentials store URL per-instance)
                "http://localhost:8080".to_string()
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

    /// Result of a login attempt
    enum LoginResult {
        Success(LoginResponse),
        SetupRequired,
        Failed(String),
    }

    // Helper function to exchange username/password for JWT token
    async fn try_login(
        server_url: &str,
        username: &str,
        password: &str,
        verbose: bool,
    ) -> LoginResult {
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
                return LoginResult::Failed(e.to_string());
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
                LoginResult::Success(response)
            },
            Err(KalamLinkError::SetupRequired(msg)) => {
                if verbose {
                    eprintln!("Server requires setup: {}", msg);
                }
                LoginResult::SetupRequired
            },
            Err(e) => {
                if verbose {
                    eprintln!("Warning: Login failed: {}", e);
                }
                LoginResult::Failed(e.to_string())
            },
        }
    }

    /// Run the server setup wizard
    /// 
    /// Returns the username and password that were set up so the caller can login.
    async fn run_setup_wizard(server_url: &str) -> std::result::Result<(String, String), String> {
        println!();
        println!("╔═══════════════════════════════════════════════════════════════════╗");
        println!("║                    KalamDB Server Setup                           ║");
        println!("╠═══════════════════════════════════════════════════════════════════╣");
        println!("║                                                                   ║");
        println!("║  This server requires initial setup. You will need to:            ║");
        println!("║  1. Set a root password (for system administration)               ║");
        println!("║  2. Create your DBA user account                                  ║");
        println!("║                                                                   ║");
        println!("╚═══════════════════════════════════════════════════════════════════╝");
        println!();

        // Get DBA username
        print!("Enter username for your DBA account: ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut username = String::new();
        io::stdin().read_line(&mut username).map_err(|e| e.to_string())?;
        let username = username.trim().to_string();
        if username.is_empty() {
            return Err("Username cannot be empty".to_string());
        }
        if username.to_lowercase() == "root" {
            return Err("Cannot use 'root' as username. Choose a different name.".to_string());
        }

        // Get DBA password
        let password = rpassword::prompt_password("Enter password for your DBA account: ")
            .map_err(|e| e.to_string())?;
        if password.is_empty() {
            return Err("Password cannot be empty".to_string());
        }

        // Confirm password
        let password_confirm = rpassword::prompt_password("Confirm password: ")
            .map_err(|e| e.to_string())?;
        if password != password_confirm {
            return Err("Passwords do not match".to_string());
        }

        // Get root password
        println!();
        println!("Now set the root password (for system administration):");
        let root_password = rpassword::prompt_password("Enter root password: ")
            .map_err(|e| e.to_string())?;
        if root_password.is_empty() {
            return Err("Root password cannot be empty".to_string());
        }

        // Confirm root password
        let root_password_confirm = rpassword::prompt_password("Confirm root password: ")
            .map_err(|e| e.to_string())?;
        if root_password != root_password_confirm {
            return Err("Root passwords do not match".to_string());
        }

        // Optional email
        print!("Enter email (optional, press Enter to skip): ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut email = String::new();
        io::stdin().read_line(&mut email).map_err(|e| e.to_string())?;
        let email = email.trim().to_string();
        let email = if email.is_empty() { None } else { Some(email) };

        println!();
        println!("Setting up server...");

        // Create client and call setup endpoint
        let client = KalamLinkClient::builder()
            .base_url(server_url)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;

        let request = ServerSetupRequest::new(username.clone(), password.clone(), root_password, email);

        match client.server_setup(request).await {
            Ok(_response) => {
                println!();
                println!("╔═══════════════════════════════════════════════════════════════════╗");
                println!("║                    Setup Complete!                                ║");
                println!("╠═══════════════════════════════════════════════════════════════════╣");
                println!("║                                                                   ║");
                println!("║  ✓ Root password has been set                                     ║");
                println!(
                    "║  ✓ DBA user '{}' has been created{} ║",
                    username,
                    " ".repeat(36 - username.len().min(36))
                );
                println!("║                                                                   ║");
                println!("║  Please login with your new credentials.                          ║");
                println!("║                                                                   ║");
                println!("╚═══════════════════════════════════════════════════════════════════╝");
                println!();

                // Return the credentials so the caller can login
                Ok((username, password))
            },
            Err(e) => Err(format!("Setup failed: {}", e)),
        }
    }

    /// Perform setup and login with created credentials
    async fn setup_and_login(
        server_url: &str,
        verbose: bool,
        instance: &str,
        credential_store: &mut FileCredentialStore,
        save_credentials: bool,
    ) -> Result<(AuthProvider, Option<String>, bool)> {
        match run_setup_wizard(server_url).await {
            Ok((setup_username, setup_password)) => {
                match try_login(server_url, &setup_username, &setup_password, verbose).await {
                    LoginResult::Success(login_response) => {
                        let authenticated_user = login_response.user.username.clone();

                        if save_credentials {
                            let new_creds = Credentials::with_refresh_token(
                                instance.to_string(),
                                login_response.access_token.clone(),
                                login_response.user.username.clone(),
                                login_response.expires_at.clone(),
                                Some(server_url.to_string()),
                                login_response.refresh_token.clone(),
                                login_response.refresh_expires_at.clone(),
                            );
                            let _ = credential_store.set_credentials(&new_creds);
                        }

                        Ok((
                            AuthProvider::jwt_token(login_response.access_token),
                            Some(authenticated_user),
                            false,
                        ))
                    },
                    _ => Err(CLIError::SetupRequired(
                        "Setup completed but login failed. Please try logging in manually.".to_string(),
                    )),
                }
            },
            Err(e) => Err(CLIError::SetupRequired(e)),
        }
    }

    /// Prompt user for credentials and attempt login (interactive only)
    async fn prompt_and_login(
        server_url: &str,
        verbose: bool,
        instance: &str,
        credential_store: &mut FileCredentialStore,
    ) -> Result<(AuthProvider, Option<String>, bool)> {
        println!();
        println!("No authentication credentials found.");
        println!("Please enter your credentials to connect:");
        println!();

        // Prompt for username
        print!("Username: ");
        io::stdout().flush().map_err(|e| {
            CLIError::FileError(format!("Failed to flush stdout: {}", e))
        })?;
        let mut username = String::new();
        io::stdin().read_line(&mut username).map_err(|e| {
            CLIError::FileError(format!("Failed to read username: {}", e))
        })?;
        let username = username.trim().to_string();

        if username.is_empty() {
            return Err(CLIError::ConfigurationError(
                "Username cannot be empty".to_string(),
            ));
        }

        // Prompt for password
        let password = rpassword::prompt_password("Password: ").map_err(|e| {
            CLIError::FileError(format!("Failed to read password: {}", e))
        })?;

        // Try to login with provided credentials
        match try_login(server_url, &username, &password, verbose).await {
            LoginResult::Success(login_response) => {
                let authenticated_user = login_response.user.username.clone();

                // Ask if user wants to save credentials
                print!("\nSave credentials for future use? (y/N): ");
                io::stdout().flush().ok();
                let mut save_choice = String::new();
                io::stdin().read_line(&mut save_choice).ok();

                if save_choice.trim().eq_ignore_ascii_case("y")
                    || save_choice.trim().eq_ignore_ascii_case("yes")
                {
                    let new_creds = Credentials::with_refresh_token(
                        instance.to_string(),
                        login_response.access_token.clone(),
                        login_response.user.username.clone(),
                        login_response.expires_at.clone(),
                        Some(server_url.to_string()),
                        login_response.refresh_token.clone(),
                        login_response.refresh_expires_at.clone(),
                    );

                    if let Err(e) = credential_store.set_credentials(&new_creds) {
                        eprintln!("Warning: Could not save credentials: {}", e);
                    } else {
                        println!("Credentials saved for instance '{}'", instance);
                    }
                }

                println!();
                Ok((
                    AuthProvider::jwt_token(login_response.access_token),
                    Some(authenticated_user),
                    false,
                ))
            },
            LoginResult::SetupRequired => {
                setup_and_login(server_url, verbose, instance, credential_store, true).await
            },
            LoginResult::Failed(_) => Err(CLIError::ConfigurationError(
                "Login failed: invalid username or password".to_string(),
            )),
        }
    }

    // Helper function to refresh access token using refresh token
    async fn try_refresh_token(
        server_url: &str,
        refresh_token: &str,
        verbose: bool,
    ) -> Option<LoginResponse> {
        let temp_client = match KalamLinkClient::builder()
            .base_url(server_url)
            .timeout(Duration::from_secs(10))
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                if verbose {
                    eprintln!("Warning: Could not create client for token refresh: {}", e);
                }
                return None;
            },
        };

        match temp_client.refresh_access_token(refresh_token).await {
            Ok(response) => {
                if verbose {
                    eprintln!(
                        "Successfully refreshed token for '{}' (expires: {})",
                        response.user.username, response.expires_at
                    );
                }
                Some(response)
            },
            Err(e) => {
                if verbose {
                    eprintln!("Warning: Token refresh failed: {}", e);
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
        // If password is missing and terminal is available, prompt for it
        let password = if let Some(pwd) = cli.password.clone() {
            pwd
        } else if std::io::stdin().is_terminal() {
            println!();
            println!("Username: {}", username);
            rpassword::prompt_password("Password: ").map_err(|e| {
                CLIError::FileError(format!("Failed to read password: {}", e))
            })?
        } else {
            // Non-interactive mode without password - use empty password
            String::new()
        };

        match try_login(&server_url, &username, &password, cli.verbose).await {
            LoginResult::Success(login_response) => {
                let authenticated_user = login_response.user.username.clone();

                // Only save credentials if --save-credentials flag is set
                if cli.save_credentials {
                    let new_creds = Credentials::with_refresh_token(
                        cli.instance.clone(),
                        login_response.access_token.clone(),
                        login_response.user.username.clone(),
                        login_response.expires_at.clone(),
                        Some(server_url.clone()),
                        login_response.refresh_token.clone(),
                        login_response.refresh_expires_at.clone(),
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
            },
            LoginResult::SetupRequired => {
                setup_and_login(
                    &server_url,
                    cli.verbose,
                    &cli.instance,
                    credential_store,
                    cli.save_credentials,
                )
                .await?
            },
            LoginResult::Failed(_) => {
                return Err(CLIError::ConfigurationError(
                    "Login failed: invalid username or password".to_string(),
                ));
            }
        }
    } else if let Some(creds) = credential_store
        .get_credentials(&cli.instance)
        .map_err(|e| CLIError::ConfigurationError(format!("Failed to load credentials: {}", e)))?
    {
        // Load from stored credentials (JWT token)
        if creds.is_expired() {
            // Access token expired - try to refresh using refresh_token
            eprintln!(
                "Warning: Stored credentials for '{}' have expired.",
                cli.instance
            );

            // Try to refresh the token if we have a valid refresh token
            if creds.can_refresh() {
                if cli.verbose {
                    eprintln!("Attempting to refresh access token...");
                }

                let refresh_server_url = creds.server_url.clone().unwrap_or(server_url.clone());
                if let Some(login_response) =
                    try_refresh_token(&refresh_server_url, creds.refresh_token.as_ref().unwrap(), cli.verbose).await
                {
                    // Save the refreshed credentials
                    let new_creds = Credentials::with_refresh_token(
                        cli.instance.clone(),
                        login_response.access_token.clone(),
                        login_response.user.username.clone(),
                        login_response.expires_at.clone(),
                        Some(refresh_server_url),
                        login_response.refresh_token.clone(),
                        login_response.refresh_expires_at.clone(),
                    );

                    if let Err(e) = credential_store.set_credentials(&new_creds) {
                        if cli.verbose {
                            eprintln!("Warning: Could not save refreshed credentials: {}", e);
                        }
                    } else {
                        eprintln!("Successfully refreshed credentials for instance '{}'", cli.instance);
                    }

                    let authenticated_user = login_response.user.username.clone();
                    (
                        AuthProvider::jwt_token(login_response.access_token),
                        Some(authenticated_user),
                        true,
                    )
                } else {
                    // Refresh failed - warn and fall back to localhost auto-auth or no auth
                    eprintln!(
                        "Warning: Could not refresh token. Please login again with --username and --password --save-credentials"
                    );

                    // Fall through to localhost auto-auth or no auth
                    if is_localhost_url(&server_url) {
                        let username = "root".to_string();
                        let password = "".to_string();

                        match try_login(&server_url, &username, &password, cli.verbose).await {
                            LoginResult::Success(login_response) => {
                                eprintln!("Auto-authenticated as root for localhost connection");
                                (
                                    AuthProvider::jwt_token(login_response.access_token),
                                    Some(login_response.user.username),
                                    false,
                                )
                            },
                            LoginResult::SetupRequired => {
                                // Run setup wizard then login
                                match run_setup_wizard(&server_url).await {
                                    Ok((setup_username, setup_password)) => {
                                        match try_login(&server_url, &setup_username, &setup_password, cli.verbose).await {
                                            LoginResult::Success(login_response) => (
                                                AuthProvider::jwt_token(login_response.access_token),
                                                Some(login_response.user.username),
                                                false,
                                            ),
                                            _ => {
                                                return Err(CLIError::SetupRequired(
                                                    "Setup completed but login failed.".to_string()
                                                ));
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        return Err(CLIError::SetupRequired(e));
                                    }
                                }
                            },
                            LoginResult::Failed(_) => {
                                (AuthProvider::None, None, false)
                            }
                        }
                    } else {
                        (AuthProvider::None, None, false)
                    }
                }
            } else {
                // No refresh token available - warn and fall back
                eprintln!(
                    "Warning: No refresh token available. Please login again with --username and --password --save-credentials"
                );

                // Fall through to localhost auto-auth or no auth
                if is_localhost_url(&server_url) {
                    let username = "root".to_string();
                    let password = "".to_string();

                    match try_login(&server_url, &username, &password, cli.verbose).await {
                        LoginResult::Success(login_response) => {
                            eprintln!("Auto-authenticated as root for localhost connection");
                            (
                                AuthProvider::jwt_token(login_response.access_token),
                                Some(login_response.user.username),
                                false,
                            )
                        },
                        LoginResult::SetupRequired => {
                            // Run setup wizard then login
                            match run_setup_wizard(&server_url).await {
                                Ok((setup_username, setup_password)) => {
                                    match try_login(&server_url, &setup_username, &setup_password, cli.verbose).await {
                                        LoginResult::Success(login_response) => (
                                            AuthProvider::jwt_token(login_response.access_token),
                                            Some(login_response.user.username),
                                            false,
                                        ),
                                        _ => {
                                            return Err(CLIError::SetupRequired(
                                                "Setup completed but login failed.".to_string()
                                            ));
                                        }
                                    }
                                },
                                Err(e) => {
                                    return Err(CLIError::SetupRequired(e));
                                }
                            }
                        },
                        LoginResult::Failed(_) => {
                            (AuthProvider::None, None, false)
                        }
                    }
                } else {
                    (AuthProvider::None, None, false)
                }
            }
        } else {
            // Token is still valid
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
        }
    } else {
        // No credentials provided - prompt interactively if terminal is available
        if std::io::stdin().is_terminal() {
            prompt_and_login(
                &server_url,
                cli.verbose,
                &cli.instance,
                credential_store,
            )
            .await?
        } else if is_localhost_url(&server_url) {
            // Non-interactive mode on localhost - try root auto-auth
            let username = "root".to_string();
            let password = "".to_string();

            match try_login(&server_url, &username, &password, cli.verbose).await {
            LoginResult::Success(login_response) => {
                if cli.verbose {
                    eprintln!("Auto-authenticated as root for localhost connection");
                }
                (
                    AuthProvider::jwt_token(login_response.access_token),
                    Some(login_response.user.username),
                    false,
                )
            },
            LoginResult::SetupRequired => {
                // Server requires initial setup - run the setup wizard then login
                match run_setup_wizard(&server_url).await {
                    Ok((setup_username, setup_password)) => {
                        match try_login(&server_url, &setup_username, &setup_password, cli.verbose).await {
                            LoginResult::Success(login_response) => {
                                let authenticated_user = login_response.user.username.clone();
                                
                                // Save credentials after successful setup
                                let new_creds = Credentials::with_refresh_token(
                                    cli.instance.clone(),
                                    login_response.access_token.clone(),
                                    login_response.user.username.clone(),
                                    login_response.expires_at.clone(),
                                    Some(server_url.clone()),
                                    login_response.refresh_token.clone(),
                                    login_response.refresh_expires_at.clone(),
                                );
                                let _ = credential_store.set_credentials(&new_creds);
                                
                                (
                                    AuthProvider::jwt_token(login_response.access_token),
                                    Some(authenticated_user),
                                    false,
                                )
                            },
                            _ => {
                                return Err(CLIError::SetupRequired(
                                    "Setup completed but login failed. Please try logging in manually.".to_string()
                                ));
                            }
                        }
                    },
                    Err(e) => {
                        return Err(CLIError::SetupRequired(e));
                    }
                }
            },
            LoginResult::Failed(e) => {
                if cli.verbose {
                    eprintln!("Auto-login failed: {}", e);
                }
                (AuthProvider::None, None, false)
            }
        }
        } else {
            // Non-interactive mode and not localhost - no auth available
            return Err(CLIError::ConfigurationError(
                "No authentication credentials available. Use --username and --password, or run interactively.".to_string()
            ));
        }
    };

    let mut connection_options = config.to_connection_options();
    if server_url.starts_with("http://") {
        use kalam_link::HttpVersion;
        if connection_options.http_version == HttpVersion::Http2 {
            connection_options = connection_options.with_http_version(HttpVersion::Auto);
        }
    }

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
        Some(connection_options),
        config.clone(),
        config_path,
        credentials_loaded,
    )
    .await
}
