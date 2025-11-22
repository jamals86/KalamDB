use crate::args::Cli;
use kalam_cli::{CLIError, FileCredentialStore, Result};
use kalam_link::credentials::{CredentialStore, Credentials};
use std::io::{self, Write};

pub fn handle_credentials(cli: &Cli, credential_store: &mut FileCredentialStore) -> Result<bool> {
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
        return Ok(true);
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
        return Ok(true);
    }

    if cli.delete_credentials {
        credential_store
            .delete_credentials(&cli.instance)
            .map_err(|e| {
                CLIError::ConfigurationError(format!("Failed to delete credentials: {}", e))
            })?;
        println!("Deleted credentials for instance '{}'", cli.instance);
        return Ok(true);
    }

    if cli.update_credentials {
        // Prompt for credentials
        let username = if let Some(user) = &cli.username {
            user.clone()
        } else {
            // Read from stdin
            print!("Username: ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .map_err(|e| CLIError::FileError(format!("Failed to read username: {}", e)))?;
            input.trim().to_string()
        };

        let password = if let Some(pass) = &cli.password {
            pass.clone()
        } else {
            // Use rpassword for secure password input
            rpassword::prompt_password("Password: ")
                .map_err(|e| CLIError::FileError(format!("Failed to read password: {}", e)))?
        };

        let server_url = cli.url.clone().or_else(|| {
            cli.host
                .as_ref()
                .map(|h| format!("http://{}:{}", h, cli.port))
        });

        let creds = if let Some(url) = server_url {
            Credentials::with_server_url(cli.instance.clone(), username, password, url)
        } else {
            Credentials::new(cli.instance.clone(), username, password)
        };

        credential_store.set_credentials(&creds).map_err(|e| {
            CLIError::ConfigurationError(format!("Failed to save credentials: {}", e))
        })?;
        println!("Saved credentials for instance '{}'", cli.instance);
        return Ok(true);
    }

    Ok(false)
}
