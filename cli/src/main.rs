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
use std::io::IsTerminal;

use kalam_cli::{CLIConfiguration, CLIError, FileCredentialStore, Result};

mod args;
mod commands;
mod connect;

use args::Cli;
use commands::credentials::{handle_credentials, login_and_store_credentials};
use commands::subscriptions::handle_subscriptions;
use connect::create_session;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        // Use Display formatting instead of Debug to show nice error messages
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Parse command-line arguments
    let mut cli = Cli::parse();

    // If the password is explicitly set to an empty string, only prompt in interactive mode.
    // In non-interactive modes (--command/--file), an empty password may be valid (e.g. default root).
    let is_interactive_mode = cli.command.is_none() && cli.file.is_none();
    if cli.password.as_deref() == Some("") && is_interactive_mode && std::io::stdin().is_terminal() {
        let password = rpassword::prompt_password("Password: ")
            .map_err(|e| CLIError::FileError(format!("Failed to read password: {}", e)))?;
        cli.password = Some(password);
    }

    // Initialize logging (basic)
    if cli.verbose {
        eprintln!("Verbose mode enabled");
    }

    // Load credential store
    let mut credential_store = FileCredentialStore::new()?;

    // Handle credential management commands (sync operations like list, show, delete)
    if handle_credentials(&cli, &mut credential_store)? {
        return Ok(());
    }

    // Handle credential login/update (async - requires network)
    if login_and_store_credentials(&cli, &mut credential_store).await? {
        return Ok(());
    }

    // Handle subscription management commands
    if handle_subscriptions(&cli, &mut credential_store).await? {
        return Ok(());
    }

    // Load configuration
    let config = CLIConfiguration::load(&cli.config)?;

    let mut session = create_session(&cli, &mut credential_store, &config).await?;

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
