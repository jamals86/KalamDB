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

use kalam_cli::{CLIConfiguration, CLIError, FileCredentialStore, Result};

mod args;
mod commands;
mod connect;

use args::Cli;
use commands::credentials::handle_credentials;
use commands::subscriptions::handle_subscriptions;
use connect::create_session;

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
    if handle_credentials(&cli, &mut credential_store)? {
        return Ok(());
    }

    // Handle subscription management commands
    if handle_subscriptions(&cli, &credential_store).await? {
        return Ok(());
    }

    // Load configuration
    let config = CLIConfiguration::load(&cli.config)?;

    let mut session = create_session(&cli, &credential_store, &config).await?;

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
