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
use std::path::PathBuf;

use kalam_cli::{CLIConfiguration, CLIError, CLISession, OutputFormat, Result};

/// Kalam CLI - Terminal client for KalamDB
#[derive(Parser, Debug)]
#[command(name = "kalam")]
#[command(author = "KalamDB Team")]
#[command(version = env!("CARGO_PKG_VERSION"))]
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

    /// API key authentication
    #[arg(long = "apikey")]
    api_key: Option<String>,

    /// User ID for X-USER-ID header
    #[arg(long = "user-id")]
    user_id: Option<String>,

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
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let cli = Cli::parse();

    // Initialize logging (basic)
    if cli.verbose {
        eprintln!("Verbose mode enabled");
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
    let server_url = match (cli.url, cli.host) {
        (Some(url), _) => url,
        (None, Some(host)) => format!("http://{}:{}", host, cli.port),
        (None, None) => config
            .server
            .as_ref()
            .and_then(|s| s.url.clone())
            .ok_or_else(|| {
                CLIError::ConfigurationError(
                    "Server URL not specified. Use -u or -h flag, or set in config file.".into(),
                )
            })?,
    };

    // Create CLI session
    let jwt_token = cli
        .token
        .or_else(|| config.auth.as_ref().and_then(|a| a.jwt_token.clone()));
    let api_key = cli
        .api_key
        .or_else(|| config.auth.as_ref().and_then(|a| a.api_key.clone()));
    let user_id = cli.user_id;

    let mut session = CLISession::new(
        server_url,
        jwt_token,
        api_key,
        user_id,
        format,
        !cli.no_color,
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
