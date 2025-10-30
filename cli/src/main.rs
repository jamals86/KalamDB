//! Kalam CLI - Terminal client for KalamDB
//!
//! **Implements T083**: CLI entry point with argument parsing using clap 4.4
//!
//! # Usage
//!
//! ```bash
//! # Interactive mode
//! kalam-cli -u http://localhost:3000 --token <JWT>
//!
//! # Execute SQL file
//! kalam-cli -u http://localhost:3000 --file queries.sql
//!
//! # JSON output
//! kalam-cli -u http://localhost:3000 --json -c "SELECT * FROM users"
//! ```

use clap::Parser;
use kalam_link::credentials::{CredentialStore, Credentials};
use kalam_link::AuthProvider;
use std::path::PathBuf;

use kalam_cli::{
    CLIConfiguration, CLIError, CLISession, FileCredentialStore, OutputFormat, Result,
};

// Build information - Create a static version string at compile time
const VERSION: &str = env!("CARGO_PKG_VERSION");
const COMMIT: &str = env!("GIT_COMMIT_HASH");
const BUILD_DATE: &str = env!("BUILD_DATE");
const BRANCH: &str = env!("GIT_BRANCH");

// Macro to create the version string at compile time
macro_rules! version_string {
    () => {
        concat!(
            env!("CARGO_PKG_VERSION"),
            "\nCommit: ",
            env!("GIT_COMMIT_HASH"),
            " (",
            env!("GIT_BRANCH"),
            ")\nBuilt: ",
            env!("BUILD_DATE")
        )
    };
}

/// Kalam CLI - Terminal client for KalamDB
#[derive(Parser, Debug)]
#[command(name = "kalam")]
#[command(author = "KalamDB Team")]
#[command(version = version_string!())]
#[command(about = "Interactive SQL terminal for KalamDB", long_about = None)]
struct Cli {
    /// Server URL (e.g., http://localhost:3000)
    #[arg(short = 'u', long = "url")]
    url: Option<String>,

    /// Host address (alternative to URL)
    #[arg(short = 'H', long = "host")]
    host: Option<String>,

    /// Port number (default: 3000)
    #[arg(short = 'p', long = "port", default_value = "3000")]
    port: u16,

    /// JWT authentication token
    #[arg(long = "token")]
    token: Option<String>,

    /// HTTP Basic Auth username
    #[arg(long = "username")]
    username: Option<String>,

    /// HTTP Basic Auth password
    #[arg(long = "password")]
    password: Option<String>,

    /// Database instance name (for credential storage)
    #[arg(long = "instance", default_value = "local")]
    instance: String,

    /// Execute SQL from file and exit
    #[arg(short = 'f', long = "file")]
    file: Option<PathBuf>,

    /// Execute SQL command and exit
    #[arg(short = 'c', long = "command")]
    command: Option<String>,

    /// Output format
    #[arg(long = "format", default_value = "table")]
    format: OutputFormat,

    /// Enable JSON output (shorthand for --format=json)
    #[arg(long = "json", conflicts_with = "format")]
    json: bool,

    /// Enable CSV output (shorthand for --format=csv)
    #[arg(long = "csv", conflicts_with = "format")]
    csv: bool,

    /// Disable colored output
    #[arg(long = "no-color")]
    no_color: bool,

    /// Configuration file path
    #[arg(long = "config", default_value = "~/.kalam/config.toml")]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    // Credential management commands
    /// Show stored credentials for instance
    #[arg(long = "show-credentials")]
    show_credentials: bool,

    /// Update stored credentials for instance
    #[arg(long = "update-credentials")]
    update_credentials: bool,

    /// Delete stored credentials for instance
    #[arg(long = "delete-credentials")]
    delete_credentials: bool,

    /// List all stored credential instances
    #[arg(long = "list-instances")]
    list_instances: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let cli = Cli::parse();

    // Initialize logging (basic)
    if cli.verbose {
        eprintln!("Verbose mode enabled");
    }

    // Load credential store
    let mut credential_store = FileCredentialStore::new()
        .map_err(|e| CLIError::ConfigurationError(format!("Failed to load credentials: {}", e)))?;

    // Handle credential management commands
    if cli.list_instances {
        let instances = credential_store.list_instances().map_err(|e| {
            CLIError::ConfigurationError(format!("Failed to list instances: {}", e))
        })?;
        if instances.is_empty() {
            println!("No stored credentials");
        } else {
            println!("Stored credential instances:");
            for instance in instances {
                println!("  • {}", instance);
            }
        }
        return Ok(());
    }

    if cli.show_credentials {
        match credential_store
            .get_credentials(&cli.instance)
            .map_err(|e| {
                CLIError::ConfigurationError(format!("Failed to get credentials: {}", e))
            })? {
            Some(creds) => {
                println!("Instance: {}", creds.instance);
                println!("Username: {}", creds.username);
                println!("Password: ******** (hidden)");
                if let Some(url) = &creds.server_url {
                    println!("Server URL: {}", url);
                }
            }
            None => {
                println!("No credentials stored for instance '{}'", cli.instance);
            }
        }
        return Ok(());
    }

    if cli.delete_credentials {
        credential_store
            .delete_credentials(&cli.instance)
            .map_err(|e| {
                CLIError::ConfigurationError(format!("Failed to delete credentials: {}", e))
            })?;
        println!("Deleted credentials for instance '{}'", cli.instance);
        return Ok(());
    }

    if cli.update_credentials {
        // Prompt for credentials
        let username = if let Some(user) = cli.username {
            user
        } else {
            // Read from stdin
            use std::io::{self, Write};
            print!("Username: ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .map_err(|e| CLIError::FileError(format!("Failed to read username: {}", e)))?;
            input.trim().to_string()
        };

        let password = if let Some(pass) = cli.password {
            pass
        } else {
            // Use rpassword for secure password input
            rpassword::prompt_password("Password: ")
                .map_err(|e| CLIError::FileError(format!("Failed to read password: {}", e)))?
        };

        let server_url = cli
            .url
            .or_else(|| cli.host.map(|h| format!("http://{}:{}", h, cli.port)));

        let creds = if let Some(url) = server_url {
            Credentials::with_server_url(cli.instance.clone(), username, password, url)
        } else {
            Credentials::new(cli.instance.clone(), username, password)
        };

        credential_store.set_credentials(&creds).map_err(|e| {
            CLIError::ConfigurationError(format!("Failed to save credentials: {}", e))
        })?;
        println!("Saved credentials for instance '{}'", cli.instance);
        return Ok(());
    }

    // Load configuration
    let config = CLIConfiguration::load(&cli.config)?;

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

    let mut session = CLISession::with_auth_and_instance(
        server_url,
        auth,
        format,
        !cli.no_color,
        Some(cli.instance.clone()),
        Some(credential_store),
    )
    .await?;

    // Execute based on mode
    match (cli.file, cli.command) {
        // Execute SQL file
        (Some(file), None) => {
            let sql = std::fs::read_to_string(&file).map_err(|e| {
                CLIError::FileError(format!("Failed to read {}: {}", file.display(), e))
            })?;
            session.execute_batch(&sql).await?;
        }

        // Execute single command
        (None, Some(command)) => {
            session.execute(&command).await?;
        }

        // Interactive mode
        (None, None) => {
            session.run_interactive().await?;
        }

        // Invalid combination
        (Some(_), Some(_)) => {
            return Err(CLIError::ConfigurationError(
                "Cannot specify both --file and --command".into(),
            ));
        }
    }

    Ok(())
}
