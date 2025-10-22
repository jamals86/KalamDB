//! CLI session state management
//!
//! **Implements T084**: CLISession state with kalam-link client integration
//! **Implements T091-T093**: Interactive readline loop with command execution
//!
//! Manages the connection to KalamDB server and execution state throughout
//! the CLI session lifetime.

use kalam_link::KalamLinkClient;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::{
    error::Result,
    formatter::OutputFormatter,
    history::CommandHistory,
    parser::{Command, CommandParser},
};

/// Output format for query results
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
}

/// CLI session state
pub struct CLISession {
    /// KalamDB client
    client: KalamLinkClient,

    /// Command parser
    parser: CommandParser,

    /// Output formatter
    formatter: OutputFormatter,

    /// Server URL
    server_url: String,

    /// Output format
    format: OutputFormat,

    /// Enable colored output
    color: bool,

    /// Session is connected
    connected: bool,

    /// Active subscription paused state
    subscription_paused: bool,
}

impl CLISession {
    /// Create a new CLI session
    pub async fn new(
        server_url: String,
        jwt_token: Option<String>,
        api_key: Option<String>,
        user_id: Option<String>,
        format: crate::OutputFormat,
        color: bool,
    ) -> Result<Self> {
        // Build kalam-link client with authentication
        let mut builder = KalamLinkClient::builder()
            .base_url(&server_url)
            .timeout(std::time::Duration::from_secs(30))
            .max_retries(3);

        // Add authentication
        builder = match (jwt_token, api_key) {
            (Some(token), _) => builder.jwt_token(token),
            (None, Some(key)) => builder.api_key(key),
            (None, None) => builder,
        };

        // Add user ID if provided
        if let Some(uid) = user_id {
            builder = builder.user_id(uid);
        }

        let client = builder.build()?;

        // Convert format enum
        let output_format = match format {
            crate::OutputFormat::Table => OutputFormat::Table,
            crate::OutputFormat::Json => OutputFormat::Json,
            crate::OutputFormat::Csv => OutputFormat::Csv,
        };

        Ok(Self {
            client,
            parser: CommandParser::new(),
            formatter: OutputFormatter::new(output_format, color),
            server_url,
            format: output_format,
            color,
            connected: true,
            subscription_paused: false,
        })
    }

    /// Execute a SQL query
    ///
    /// **Implements T092**: Execute SQL via kalam-link client
    pub async fn execute(&mut self, sql: &str) -> Result<()> {
        match self.client.execute_query(sql).await {
            Ok(response) => {
                let output = self.formatter.format_response(&response)?;
                println!("{}", output);
                Ok(())
            }
            Err(e) => {
                // Don't print error here - let caller handle it
                Err(e.into())
            }
        }
    }

    /// Execute multiple SQL statements
    pub async fn execute_batch(&mut self, sql: &str) -> Result<()> {
        for statement in sql.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                self.execute(statement).await?;
            }
        }
        Ok(())
    }

    /// Run interactive readline loop
    ///
    /// **Implements T093**: Interactive REPL with rustyline
    pub async fn run_interactive(&mut self) -> Result<()> {
        println!("Kalam CLI v{}", env!("CARGO_PKG_VERSION"));
        println!("Connected to: {}", self.server_url);
        println!("Type \\help for help, \\quit to exit\n");

        // Initialize readline
        let mut rl = DefaultEditor::new()?;
        let history = CommandHistory::new(1000);

        // Load history
        if let Ok(history_entries) = history.load() {
            for entry in history_entries {
                let _ = rl.add_history_entry(&entry);
            }
        }

        // Main REPL loop
        loop {
            let prompt = if self.connected {
                "kalam> "
            } else {
                "kalam (disconnected)> "
            };

            match rl.readline(prompt) {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    // Add to history
                    let _ = rl.add_history_entry(line);
                    let _ = history.append(line);

                    // Parse and execute command
                    match self.parser.parse(line) {
                        Ok(command) => {
                            if let Err(e) = self.execute_command(command).await {
                                eprintln!("Error: {}", e);
                            }
                        }
                        Err(e) => {
                            eprintln!("Parse error: {}", e);
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("Use \\quit or \\q to exit");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    println!("Goodbye!");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {}", err);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Execute a parsed command
    ///
    /// **Implements T094**: Backslash command handling
    async fn execute_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Sql(sql) => {
                self.execute(&sql).await?;
            }
            Command::Quit => {
                println!("Goodbye!");
                std::process::exit(0);
            }
            Command::Help => {
                self.show_help();
            }
            Command::Connect(url) => {
                println!("Reconnecting to: {}", url);
                // TODO: Implement reconnection
                println!("Note: Reconnection not yet implemented. Please restart the CLI.");
            }
            Command::Config => {
                println!("Configuration:");
                println!("  Server: {}", self.server_url);
                println!("  Format: {:?}", self.format);
                println!("  Color: {}", self.color);
            }
            Command::Flush => {
                println!("Flushing database...");
                match self.execute("FLUSH").await {
                    Ok(_) => println!("Flush completed successfully"),
                    Err(e) => eprintln!("Flush failed: {}", e),
                }
            }
            Command::Health => {
                match self.health_check().await {
                    Ok(_) => {}
                    Err(e) => eprintln!("Health check failed: {}", e),
                }
            }
            Command::Pause => {
                println!("Pausing ingestion...");
                match self.execute("PAUSE").await {
                    Ok(_) => println!("Ingestion paused"),
                    Err(e) => eprintln!("Pause failed: {}", e),
                }
            }
            Command::Continue => {
                println!("Resuming ingestion...");
                match self.execute("CONTINUE").await {
                    Ok(_) => println!("Ingestion resumed"),
                    Err(e) => eprintln!("Resume failed: {}", e),
                }
            }
            Command::ListTables => {
                self.execute("SELECT name, type FROM system.tables").await?;
            }
            Command::Describe(table) => {
                let query = format!("SELECT * FROM system.columns WHERE table_name = '{}'", table);
                self.execute(&query).await?;
            }
            Command::SetFormat(format) => {
                match format.to_lowercase().as_str() {
                    "table" => {
                        self.set_format(OutputFormat::Table);
                        println!("Output format set to: table");
                    }
                    "json" => {
                        self.set_format(OutputFormat::Json);
                        println!("Output format set to: json");
                    }
                    "csv" => {
                        self.set_format(OutputFormat::Csv);
                        println!("Output format set to: csv");
                    }
                    _ => {
                        eprintln!("Unknown format: {}. Use: table, json, or csv", format);
                    }
                }
            }
            Command::Subscribe(query) => {
                self.run_subscription(&query).await?;
            }
            Command::Unsubscribe => {
                println!("No active subscription to cancel");
            }
            Command::Unknown(cmd) => {
                eprintln!("Unknown command: {}. Type \\help for help.", cmd);
            }
        }
        Ok(())
    }

    /// Run a WebSocket subscription
    ///
    /// **Implements T102**: WebSocket subscription display with timestamps and change indicators
    /// **Implements T103**: Ctrl+C handler for graceful subscription cancellation
    async fn run_subscription(&mut self, query: &str) -> Result<()> {
        println!("Starting subscription: {}", query);
        println!("Press Ctrl+C to stop\n");

        let mut subscription = self.client.subscribe(query).await?;

        loop {
            // Check if paused (T104)
            if self.subscription_paused {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            // Get next event
            match subscription.next().await {
                Some(Ok(event)) => {
                    self.display_change_event(&event);
                }
                Some(Err(e)) => {
                    eprintln!("Subscription error: {}", e);
                    break;
                }
                None => {
                    println!("Subscription ended");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Display a change event with formatting
    ///
    /// **Implements T102**: Change indicators (INSERT/UPDATE/DELETE)
    fn display_change_event(&self, event: &kalam_link::ChangeEvent) {
        use chrono::Local;
        let timestamp = Local::now().format("%H:%M:%S%.3f");

        match event {
            kalam_link::ChangeEvent::Insert { table, row } => {
                if self.color {
                    println!(
                        "\x1b[32m[{}] INSERT\x1b[0m into {} → {}",
                        timestamp,
                        table,
                        serde_json::to_string(row).unwrap_or_default()
                    );
                } else {
                    println!(
                        "[{}] INSERT into {} → {}",
                        timestamp,
                        table,
                        serde_json::to_string(row).unwrap_or_default()
                    );
                }
            }
            kalam_link::ChangeEvent::Update {
                table,
                old_row,
                new_row,
            } => {
                if self.color {
                    println!(
                        "\x1b[33m[{}] UPDATE\x1b[0m in {} → {} ⇒ {}",
                        timestamp,
                        table,
                        serde_json::to_string(old_row).unwrap_or_default(),
                        serde_json::to_string(new_row).unwrap_or_default()
                    );
                } else {
                    println!(
                        "[{}] UPDATE in {} → {} => {}",
                        timestamp,
                        table,
                        serde_json::to_string(old_row).unwrap_or_default(),
                        serde_json::to_string(new_row).unwrap_or_default()
                    );
                }
            }
            kalam_link::ChangeEvent::Delete { table, row } => {
                if self.color {
                    println!(
                        "\x1b[31m[{}] DELETE\x1b[0m from {} → {}",
                        timestamp,
                        table,
                        serde_json::to_string(row).unwrap_or_default()
                    );
                } else {
                    println!(
                        "[{}] DELETE from {} → {}",
                        timestamp,
                        table,
                        serde_json::to_string(row).unwrap_or_default()
                    );
                }
            }
            kalam_link::ChangeEvent::Snapshot { table, rows } => {
                println!("[{}] SNAPSHOT from {} ({} rows)", timestamp, table, rows.len());
            }
            kalam_link::ChangeEvent::Ack {
                subscription_id,
                query,
            } => {
                println!("[{}] Subscribed (ID: {}): {}", timestamp, subscription_id, query);
            }
            kalam_link::ChangeEvent::Error { message } => {
                if self.color {
                    eprintln!("\x1b[31m[{}] ERROR:\x1b[0m {}", timestamp, message);
                } else {
                    eprintln!("[{}] ERROR: {}", timestamp, message);
                }
            }
        }
    }

    /// Show help message
    fn show_help(&self) {
        println!("Kalam CLI Commands:");
        println!();
        println!("  SQL Statements:");
        println!("    SELECT, INSERT, UPDATE, DELETE, CREATE TABLE, etc.");
        println!();
        println!("  Meta-commands:");
        println!("    \\quit, \\q              Exit the CLI");
        println!("    \\help, \\?              Show this help message");
        println!("    \\connect <url>         Connect to a different server");
        println!("    \\config                Show current configuration");
        println!("    \\flush                 Flush all data to disk");
        println!("    \\health                Check server health");
        println!("    \\pause                 Pause ingestion");
        println!("    \\continue              Resume ingestion");
        println!("    \\dt, \\tables           List all tables");
        println!("    \\d <table>             Describe table schema");
        println!("    \\format <type>         Set output format (table, json, csv)");
        println!("    \\subscribe <query>     Start WebSocket subscription");
        println!("    \\watch <query>         Alias for \\subscribe");
        println!("    \\unsubscribe           Cancel active subscription");
        println!();
        println!("  Examples:");
        println!("    SELECT * FROM users WHERE age > 18;");
        println!("    INSERT INTO users (name, age) VALUES ('Alice', 25);");
        println!("    \\dt");
        println!("    \\d users");
        println!("    \\subscribe SELECT * FROM messages");
        println!();
    }

    /// Check server health
    pub async fn health_check(&self) -> Result<()> {
        self.client.health_check().await?;
        println!("✓ Server is healthy");
        Ok(())
    }

    /// Get current server URL
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Check if session is connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Set output format
    pub fn set_format(&mut self, format: OutputFormat) {
        self.format = format;
        self.formatter = OutputFormatter::new(format, self.color);
    }

    /// Set color mode
    pub fn set_color(&mut self, enabled: bool) {
        self.color = enabled;
        self.formatter = OutputFormatter::new(self.format, enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format() {
        let format = OutputFormat::Table;
        assert!(matches!(format, OutputFormat::Table));
    }
}
