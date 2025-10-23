//! CLI session state management
//!
//! **Implements T084**: CLISession state with kalam-link client integration
//! **Implements T091-T093**: Interactive readline loop with command execution
//! **Implements T114a**: Loading indicator for long-running queries
//!
//! Manages the connection to KalamDB server and execution state throughout
//! the CLI session lifetime.

use kalam_link::KalamLinkClient;
use clap::ValueEnum;
use rustyline::error::ReadlineError;
use rustyline::{Editor, Helper, Config, CompletionType, EditMode};
use rustyline::history::DefaultHistory;
use rustyline::completion::Completer;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use colored::*;

use crate::{
    error::Result,
    formatter::OutputFormatter,
    history::CommandHistory,
    parser::{Command, CommandParser},
    completer::AutoCompleter,
};

/// Output format for query results
#[derive(Debug, Clone, Copy, ValueEnum)]
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

    /// Threshold for showing loading indicator (milliseconds)
    loading_threshold_ms: u64,

    /// Current user ID
    user_id: String,

    /// Server version
    server_version: Option<String>,

    /// Server API version
    server_api_version: Option<String>,
}

impl CLISession {
    /// Create a new CLI session
    pub async fn new(
        server_url: String,
        jwt_token: Option<String>,
        api_key: Option<String>,
        user_id: Option<String>,
        format: OutputFormat,
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

        // Set user ID (default to "cli" if not provided)
        let actual_user_id = user_id.unwrap_or_else(|| "cli".to_string());
        builder = builder.user_id(actual_user_id.clone());

        let client = builder.build()?;

        // Try to fetch server info from health check
        let (server_version, server_api_version) = match client.health_check().await {
            Ok(health) => (Some(health.version), Some(health.api_version)),
            Err(_) => (None, None),
        };

        Ok(Self {
            client,
            parser: CommandParser::new(),
            formatter: OutputFormatter::new(format, color),
            server_url,
            format,
            color,
            connected: true,
            subscription_paused: false,
            loading_threshold_ms: 200, // Default: 200ms
            user_id: actual_user_id,
            server_version,
            server_api_version,
        })
    }

    /// Execute a SQL query with loading indicator
    ///
    /// **Implements T092**: Execute SQL via kalam-link client
    /// **Implements T114a**: Show loading indicator for queries > threshold
    /// **Enhanced**: Colored output and styled timing
    pub async fn execute(&mut self, sql: &str) -> Result<()> {
        let start = Instant::now();
        
        // Create a loading indicator with proper cleanup
        let spinner = Arc::new(Mutex::new(None::<ProgressBar>));
        let spinner_clone = Arc::clone(&spinner);
        
        let show_loading = tokio::spawn({
            let threshold = Duration::from_millis(self.loading_threshold_ms);
            async move {
                tokio::time::sleep(threshold).await;
                let pb = Self::create_spinner();
                *spinner_clone.lock().unwrap() = Some(pb);
            }
        });

        // Execute the query
        let result = self.client.execute_query(sql).await;
        
        // Cancel the loading indicator and finish spinner if it was shown
        show_loading.abort();
        if let Some(pb) = spinner.lock().unwrap().take() {
            pb.finish_and_clear();
        }
        
        let elapsed = start.elapsed();
        
        match result {
            Ok(response) => {
                let output = self.formatter.format_response(&response)?;
                println!("{}", output);
                
                // Show timing if query took significant time
                if elapsed.as_millis() >= self.loading_threshold_ms as u128 {
                    let timing = format!("⏱  Time: {:.3} ms", elapsed.as_secs_f64() * 1000.0);
                    if self.color {
                        println!("{}", timing.dimmed());
                    } else {
                        println!("{}", timing);
                    }
                }
                
                Ok(())
            }
            Err(e) => {
                // Don't print error here - let caller handle it
                Err(e.into())
            }
        }
    }

    /// Create a spinner for long-running operations
    fn create_spinner() -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Executing query...");
        pb.enable_steady_tick(Duration::from_millis(80));
        pb
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

    /// Run interactive readline loop with autocomplete
    ///
    /// **Implements T093**: Interactive REPL with rustyline
    /// **Implements T114b**: Enhanced autocomplete with table names
    /// **Enhanced**: Simple, fast UI without performance issues
    pub async fn run_interactive(&mut self) -> Result<()> {
        // Print welcome banner
        self.print_banner();

        // Create autocompleter and fetch initial table names
        let mut completer = AutoCompleter::new();
        print!("{}", "Fetching tables... ".dimmed());
        std::io::Write::flush(&mut std::io::stdout()).ok();
        
        if let Err(e) = self.refresh_tables(&mut completer).await {
            println!("{}", format!("⚠ {}", e).yellow());
        } else {
            println!("{}", "✓".green());
        }

        // Create rustyline helper with autocomplete only (no highlighting for performance)
        let helper = CLIHelper { 
            completer,
        };

        // Initialize readline with completer and proper configuration
        let config = Config::builder()
            .completion_type(CompletionType::List) // Show list of completions
            .completion_prompt_limit(100) // Show up to 100 completions
            .edit_mode(EditMode::Emacs) // Emacs keybindings (Tab for completion)
            .auto_add_history(true)
            .build();
            
        let mut rl = Editor::<CLIHelper, DefaultHistory>::with_config(config)?;
        rl.set_helper(Some(helper));
        
        let history = CommandHistory::new(1000);

        // Load history
        if let Ok(history_entries) = history.load() {
            for entry in history_entries {
                let _ = rl.add_history_entry(&entry);
            }
        }

        // Main REPL loop
        loop {
            // Simple prompt without ANSI codes to avoid cursor position issues
            let prompt = "kalam> ";

            match rl.readline(&prompt) {
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
                            // Handle refresh-tables command specially to update completer
                            if matches!(command, Command::RefreshTables) {
                                if let Some(helper) = rl.helper_mut() {
                                    print!("{}", "Fetching tables... ".dimmed());
                                    std::io::Write::flush(&mut std::io::stdout()).ok();
                                    
                                    if let Err(e) = self.refresh_tables(&mut helper.completer).await {
                                        println!("{}", format!("✗ {}", e).red());
                                    } else {
                                        println!("{}", "✓".green());
                                    }
                                }
                                continue;
                            }

                            if let Err(e) = self.execute_command(command).await {
                                eprintln!("{}", format!("✗ {}", e).red());
                            }
                        }
                        Err(e) => {
                            eprintln!("{}", format!("✗ {}", e).red());
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("{}", "Use \\quit or \\q to exit".dimmed());
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    println!("\n{}", "Goodbye!".cyan());
                    break;
                }
                Err(err) => {
                    eprintln!("{}", format!("✗ {}", err).red());
                    break;
                }
            }
        }

        Ok(())
    }

    /// Print welcome banner
    fn print_banner(&self) {
        // Removed clear screen to avoid terminal refresh issues
        println!();
        println!("{}", "╔═══════════════════════════════════════════════════════════╗".bright_blue().bold());
        println!("{}", "║                                                           ║".bright_blue().bold());
        println!("{}{}{}",
            "║        ".bright_blue().bold(),
            "🗄️  Kalam CLI - Interactive Database Terminal".white().bold(),
            "      ║".bright_blue().bold()
        );
        println!("{}", "║                                                           ║".bright_blue().bold());
        println!("{}", "╚═══════════════════════════════════════════════════════════╝".bright_blue().bold());
        println!();
        println!("  {}  {}", "📡".dimmed(), format!("Connected to: {}", self.server_url).cyan());
        println!("  {}  {}", "�".dimmed(), format!("User: {}", self.user_id).cyan());
        
        if let Some(ref version) = self.server_version {
            println!("  {}  {}", "🏷️ ".dimmed(), format!("Server version: {}", version).dimmed());
        }
        
        println!("  {}  {}", "�📚".dimmed(), format!("CLI version: {}", env!("CARGO_PKG_VERSION")).dimmed());
        println!("  {}  Type {} for help, {} to exit", "💡".dimmed(), "\\help".cyan().bold(), "\\quit".cyan().bold());
        println!();
    }

    /// Fetch table names from server and update completer
    async fn refresh_tables(&mut self, completer: &mut AutoCompleter) -> Result<()> {
        // Query system.tables to get table names
        let response = self.client.execute_query("SELECT table_name FROM system.tables").await?;
        
        // Extract table names from response
        let mut table_names = Vec::new();
        if !response.results.is_empty() {
            if let Some(rows) = &response.results[0].rows {
                for row in rows {
                    if let Some(name_value) = row.get("table_name") {
                        if let Some(name) = name_value.as_str() {
                            table_names.push(name.to_string());
                        }
                    }
                }
            }
        }
        
        completer.set_tables(table_names);
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
                self.execute("SELECT table_name, table_type FROM system.tables").await?;
            }
            Command::RefreshTables => {
                // This is handled in run_interactive, shouldn't reach here
                println!("Table names refreshed");
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
        println!("    \\refresh-tables        Refresh table names for autocomplete");
        println!();
        println!("  Features:");
        println!("    - TAB completion for SQL keywords, table names, and columns");
        println!("    - Loading indicator for queries taking longer than 200ms");
        println!("    - Command history (saved in ~/.kalam/history)");
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

/// Rustyline helper with autocomplete (no highlighting for performance)
struct CLIHelper {
    completer: AutoCompleter,
}

impl Completer for CLIHelper {
    type Candidate = <AutoCompleter as Completer>::Candidate;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Hinter for CLIHelper {
    type Hint = String;
}

impl Highlighter for CLIHelper {}

impl Validator for CLIHelper {}

impl Helper for CLIHelper {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format() {
        let format = OutputFormat::Table;
        assert!(matches!(format, OutputFormat::Table));
    }
}
