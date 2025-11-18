//! CLI session state management
//!
//! **Implements T084**: CLISession state with kalam-link client integration
//! **Implements T091-T093**: Interactive readline loop with command execution
//! **Implements T114a**: Loading indicator for long-running queries
//!
//! Manages the connection to KalamDB server and execution state throughout
//! the CLI session lifetime.

use clap::ValueEnum;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use kalam_link::{AuthProvider, KalamLinkClient, SubscriptionConfig, SubscriptionOptions};
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, EditMode, Editor, Helper};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{
    completer::{AutoCompleter, SQL_KEYWORDS, SQL_TYPES},
    error::{CLIError, Result},
    formatter::OutputFormatter,
    history::CommandHistory,
    parser::{Command, CommandParser},
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

    /// Server host (cached for prompt rendering)
    server_host: String,

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

    /// Enable spinners/animations
    animations: bool,

    /// Authenticated username
    username: String,

    /// Session start time
    connected_at: Instant,

    /// Number of queries executed in this session
    queries_executed: u64,

    /// Server version
    server_version: Option<String>,

    /// Server API version
    server_api_version: Option<String>,

    /// Server build date
    server_build_date: Option<String>,

    /// Instance name for credential management
    instance: Option<String>,

    /// Credential store for managing saved credentials
    credential_store: Option<crate::credentials::FileCredentialStore>,
}

impl CLISession {
    /// Create a new CLI session with AuthProvider
    ///
    /// **Implements T120**: Create session from stored credentials
    pub async fn with_auth(
        server_url: String,
        auth: AuthProvider,
        format: OutputFormat,
        color: bool,
    ) -> Result<Self> {
        Self::with_auth_and_instance(
            server_url, auth, format, color, None, None, None, true, None,
        )
        .await
    }

    /// Create a new CLI session with AuthProvider, instance name, and credential store
    ///
    /// **Implements T121-T122**: CLI credential management commands
    pub async fn with_auth_and_instance(
        server_url: String,
        auth: AuthProvider,
        format: OutputFormat,
        color: bool,
        instance: Option<String>,
        credential_store: Option<crate::credentials::FileCredentialStore>,
        loading_threshold_ms: Option<u64>,
        animations: bool,
        client_timeout: Option<Duration>,
    ) -> Result<Self> {
        // Build kalam-link client with authentication
        let timeout = client_timeout.unwrap_or_else(|| Duration::from_secs(30));
        let client = KalamLinkClient::builder()
            .base_url(&server_url)
            .timeout(timeout)
            .max_retries(3)
            .auth(auth.clone())
            .build()?;

        // Try to fetch server info from health check
        let (server_version, server_api_version, server_build_date, connected) =
            match client.health_check().await {
                Ok(health) => (
                    Some(health.version),
                    Some(health.api_version),
                    health.build_date,
                    true,
                ),
                Err(_) => (None, None, None, false),
            };

        // Extract username from auth provider
        let username = match &auth {
            AuthProvider::BasicAuth(username, _) => username.clone(),
            AuthProvider::JwtToken(_) => "jwt-user".to_string(),
            AuthProvider::None => "anonymous".to_string(),
        };
        let server_host = Self::extract_host(&server_url);

        Ok(Self {
            client,
            parser: CommandParser::new(),
            formatter: OutputFormatter::new(format, color),
            server_url,
            server_host,
            format,
            color,
            connected,
            subscription_paused: false,
            loading_threshold_ms: loading_threshold_ms.unwrap_or(200),
            animations,
            username,
            connected_at: Instant::now(),
            queries_executed: 0,
            server_version,
            server_api_version,
            server_build_date,
            instance,
            credential_store,
        })
    }

    /// Execute a SQL query with loading indicator
    ///
    /// **Implements T092**: Execute SQL via kalam-link client
    /// **Implements T114a**: Show loading indicator for queries > threshold
    /// **Enhanced**: Colored output and styled timing
    pub async fn execute(&mut self, sql: &str) -> Result<()> {
        let start = Instant::now();

        // Increment query counter
        self.queries_executed += 1;

        // Create a loading indicator with proper cleanup
        let spinner = Arc::new(Mutex::new(None::<ProgressBar>));
        let show_loading = if self.animations {
            let spinner_clone = Arc::clone(&spinner);
            let threshold = Duration::from_millis(self.loading_threshold_ms);
            Some(tokio::spawn(async move {
                tokio::time::sleep(threshold).await;
                let pb = Self::create_spinner();
                *spinner_clone.lock().unwrap() = Some(pb);
            }))
        } else {
            None
        };

        // Execute the query
        let result = self.client.execute_query(sql).await;

        // Cancel the loading indicator and finish spinner if it was shown
        if let Some(task) = show_loading {
            task.abort();
        }
        if let Some(pb) = spinner.lock().unwrap().take() {
            pb.finish_and_clear();
        }

        let elapsed = start.elapsed();

        match result {
            Ok(response) => {
                if let Some((config, server_message)) =
                    Self::extract_subscription_config(&response)?
                {
                    if let Some(msg) = server_message {
                        println!("{}", msg);
                    }
                    self.run_subscription(config).await?;
                    return Ok(());
                }

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

    /// Extract subscription configuration from a SUBSCRIBE TO response
    fn extract_subscription_config(
        response: &kalam_link::QueryResponse,
    ) -> Result<Option<(SubscriptionConfig, Option<String>)>> {
        if response.results.is_empty() {
            return Ok(None);
        }

        let result = &response.results[0];
        let rows = match &result.rows {
            Some(rows) if !rows.is_empty() => rows,
            _ => return Ok(None),
        };

        let row = &rows[0];
        let status = row.get("status").and_then(|v| v.as_str()).unwrap_or("");

        if status != "subscription_required" {
            return Ok(None);
        }

        let message = row
            .get("message")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let ws_url = row
            .get("ws_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let subscription_value = row.get("subscription").ok_or_else(|| {
            CLIError::ParseError("Missing subscription metadata in server response".into())
        })?;

        let subscription_obj = subscription_value.as_object().ok_or_else(|| {
            CLIError::ParseError("Subscription metadata must be a JSON object".into())
        })?;

        let sql = subscription_obj
            .get("sql")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CLIError::ParseError("Subscription metadata does not include SQL query".into())
            })?;

        let mut config = SubscriptionConfig::new(sql);

        if let Some(id) = subscription_obj.get("id").and_then(|v| v.as_str()) {
            config.id = Some(id.to_string());
        }

        if let Some(url) = ws_url {
            config.ws_url = Some(url);
        }

        if let Some(options_obj) = subscription_obj.get("options").and_then(|v| v.as_object()) {
            let mut options = SubscriptionOptions::default();
            let mut has_options = false;

            if let Some(last_rows) = options_obj.get("last_rows").and_then(|v| v.as_u64()) {
                options.last_rows = Some(last_rows as usize);
                has_options = true;
            }

            if has_options {
                config.options = Some(options);
            }
        }

        Ok(Some((config, message)))
    }

    fn extract_host(url: &str) -> String {
        let trimmed = url.trim();
        let without_scheme = trimmed
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_start_matches("ws://")
            .trim_start_matches("wss://");

        let host = without_scheme
            .split(['/', '?'])
            .next()
            .unwrap_or(without_scheme);

        if host.is_empty() {
            "localhost".to_string()
        } else {
            host.to_string()
        }
    }

    fn primary_prompt(&self) -> String {
        // On Windows, rustyline has critical issues with ANSI color codes in prompts
        // The terminal cannot properly calculate display width, causing cursor misalignment
        // Disable colors entirely in the prompt on Windows (colors still work in output)
        #[cfg(target_os = "windows")]
        let use_colors_in_prompt = false;
        #[cfg(not(target_os = "windows"))]
        let use_colors_in_prompt = self.color;

        #[cfg(target_os = "windows")]
        let use_unicode = false;
        #[cfg(not(target_os = "windows"))]
        let use_unicode = true;

        let status = if use_colors_in_prompt {
            if self.connected {
                if use_unicode {
                    "●".green().bold().to_string()
                } else {
                    "*".green().bold().to_string()
                }
            } else {
                if use_unicode {
                    "○".yellow().bold().to_string()
                } else {
                    "o".yellow().bold().to_string()
                }
            }
        } else if self.connected {
            "*".to_string()
        } else {
            "o".to_string()
        };

        let brand = if use_colors_in_prompt {
            "KalamDB".bright_blue().bold().to_string()
        } else {
            "KalamDB".to_string()
        };

        let brand_with_profile = if let Some(instance) = self.instance.as_deref() {
            if use_colors_in_prompt {
                format!("{}{}", brand, format!("[{}]", instance).dimmed())
            } else {
                format!("{}[{}]", brand, instance)
            }
        } else {
            brand
        };

        let identity = if use_colors_in_prompt {
            format!(
                "{}{}",
                self.username.cyan(),
                format!("@{}", self.server_host).dimmed()
            )
        } else {
            format!("{}@{}", self.username, self.server_host)
        };

        let arrow = if use_colors_in_prompt {
            if use_unicode {
                "❯".bright_blue().bold().to_string()
            } else {
                ">".bright_blue().bold().to_string()
            }
        } else {
            ">".to_string()
        };

        let parts = [status, brand_with_profile, identity];
        let body = parts.join(" ");
        format!("{} {} ", body, arrow)
    }

    fn continuation_prompt(&self) -> String {
        // On Windows, disable colors in prompt to avoid rustyline cursor misalignment
        #[cfg(target_os = "windows")]
        let use_colors_in_prompt = false;
        #[cfg(not(target_os = "windows"))]
        let use_colors_in_prompt = self.color;

        #[cfg(target_os = "windows")]
        let use_unicode = false;
        #[cfg(not(target_os = "windows"))]
        let use_unicode = true;

        if use_colors_in_prompt {
            if use_unicode {
                format!("  {} {} ", "↳".dimmed(), "❯".bright_blue().bold())
            } else {
                format!("  {} {} ", "->".dimmed(), ">".bright_blue().bold())
            }
        } else {
            "  -> ".to_string()
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
        // Create autocompleter and verify connection by fetching tables
        // This also verifies authentication works
        let mut completer = AutoCompleter::new();
        println!("{}", "Connecting and authenticating...".dimmed());

        // Try to fetch tables - this verifies both connection and authentication
        if let Err(e) = self.refresh_tables(&mut completer).await {
            eprintln!();
            eprintln!("{} {}", "Connection failed:".red().bold(), e);
            eprintln!();
            eprintln!("{}", "Possible issues:".yellow().bold());
            eprintln!("  • Server is not running on {}", self.server_url);
            eprintln!("  • Authentication failed (check credentials)");
            eprintln!("  • Network connectivity issue");
            eprintln!();
            eprintln!("{}", "Try:".cyan().bold());
            eprintln!(
                "  • Check if server is running: curl {}/v1/api/healthcheck",
                self.server_url
            );
            eprintln!("  • Verify credentials with: kalam --username <user> --password <pass>");
            eprintln!("  • Use \\show-credentials to see stored credentials");
            eprintln!();
            // Exit to avoid a second noisy error line from main's Result
            std::process::exit(1);
        }

        println!("{}", "✓ Connected".green());

        // Connection and auth successful - print welcome banner
        self.print_banner();

        // Create rustyline helper with autocomplete, inline hints, and syntax highlighting
        let helper = CLIHelper::new(completer, self.color);

        // Initialize readline with completer and proper configuration
        let config = Config::builder()
            .completion_type(CompletionType::List) // Show list of completions
            .completion_prompt_limit(100) // Show up to 100 completions
            .edit_mode(EditMode::Emacs) // Emacs keybindings (Tab for completion)
            .auto_add_history(false) // Manually manage history for multi-line support
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
        let mut accumulated_command = String::new();
        loop {
            // Use continuation prompt if we're accumulating a multi-line command
            let prompt = if accumulated_command.is_empty() {
                self.primary_prompt()
            } else {
                self.continuation_prompt()
            };

            match rl.readline(&prompt) {
                Ok(line) => {
                    let line = line.trim();

                    // Skip empty lines unless we're accumulating
                    if line.is_empty() && accumulated_command.is_empty() {
                        continue;
                    }

                    // Add line to accumulated command
                    if !accumulated_command.is_empty() {
                        accumulated_command.push('\n');
                    }
                    accumulated_command.push_str(line);

                    // Check if command is complete (ends with semicolon or is a backslash command)
                    let is_complete = line.ends_with(';')
                        || accumulated_command.trim_start().starts_with('\\')
                        || (line.is_empty() && !accumulated_command.is_empty());

                    if !is_complete {
                        // Need more input, continue reading
                        continue;
                    }

                    // We have a complete command - add to history as single entry
                    let final_command = accumulated_command.trim().to_string();
                    accumulated_command.clear();

                    if final_command.is_empty() {
                        continue;
                    }

                    // Add complete command to history (preserving newlines)
                    let _ = rl.add_history_entry(&final_command);
                    let _ = history.append(&final_command);

                    // Parse and execute command
                    match self.parser.parse(&final_command) {
                        Ok(command) => {
                            // Handle refresh-tables command specially to update completer
                            if matches!(command, Command::RefreshTables) {
                                if let Some(helper) = rl.helper_mut() {
                                    print!("{}", "Fetching tables... ".dimmed());
                                    std::io::Write::flush(&mut std::io::stdout()).ok();

                                    if let Err(e) = self.refresh_tables(&mut helper.completer).await
                                    {
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
                    // Clear any accumulated command on Ctrl+C
                    if !accumulated_command.is_empty() {
                        println!("\n{}", "Command cancelled".yellow());
                        accumulated_command.clear();
                    } else {
                        println!("{}", "Use \\quit or \\q to exit".dimmed());
                    }
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
        println!(
            "{}",
            "╔═══════════════════════════════════════════════════════════╗"
                .bright_blue()
                .bold()
        );
        println!(
            "{}",
            "║                                                           ║"
                .bright_blue()
                .bold()
        );
        println!(
            "{}{}{}",
            "║        ".bright_blue().bold(),
            "🗄️  Kalam CLI - Interactive Database Terminal"
                .white()
                .bold(),
            "       ║".bright_blue().bold()
        );
        println!(
            "{}",
            "║                                                           ║"
                .bright_blue()
                .bold()
        );
        println!(
            "{}",
            "╚═══════════════════════════════════════════════════════════╝"
                .bright_blue()
                .bold()
        );
        println!();
        println!(
            "  {}  {}",
            "📡".dimmed(),
            format!("Connected to: {}", self.server_url).cyan()
        );
        println!(
            "  {}  {}",
            "👤".dimmed(),
            format!("User: {}", self.username).cyan()
        );

        if let Some(ref version) = self.server_version {
            println!(
                "  {}  {}",
                "🏷️ ".dimmed(),
                format!("Server version: {}", version).dimmed()
            );
        }

        // Show CLI version with build info
        println!(
            "  {}  {}",
            "📚".dimmed(),
            format!(
                "CLI version: {} (built: {})",
                env!("CARGO_PKG_VERSION"),
                env!("BUILD_DATE")
            )
            .dimmed()
        );

        println!(
            "  {}  Type {} for help, {} for session info, {} to exit",
            "💡".dimmed(),
            "\\help".cyan().bold(),
            "\\info".cyan().bold(),
            "\\quit".cyan().bold()
        );
        println!();
    }

    /// Fetch namespaces, table names, and column names from server and update completer
    async fn refresh_tables(&mut self, completer: &mut AutoCompleter) -> Result<()> {
        // Query namespaces first
        let namespaces_res = if self.animations {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                    .template("{spinner:.cyan} Fetching namespaces...")
                    .unwrap(),
            );
            pb.enable_steady_tick(Duration::from_millis(80));
            let resp = self
                .client
                .execute_query("SELECT name FROM system.namespaces ORDER BY name")
                .await;
            pb.finish_and_clear();
            resp
        } else {
            self.client
                .execute_query("SELECT name FROM system.namespaces ORDER BY name")
                .await
        };

        if let Ok(ns_resp) = namespaces_res {
            let mut namespaces = Vec::new();
            if let Some(result) = ns_resp.results.first() {
                if let Some(rows) = &result.rows {
                    for row in rows {
                        if let Some(ns) = row.get("name").and_then(|v| v.as_str()) {
                            namespaces.push(ns.to_string());
                        }
                    }
                }
            }
            completer.set_namespaces(namespaces);
        }

        // Query system.tables to get table names and namespace mapping
        let response = if self.animations {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                    .template("{spinner:.cyan} Fetching tables...")
                    .unwrap(),
            );
            pb.enable_steady_tick(Duration::from_millis(80));
            let resp = self
                .client
                .execute_query("SELECT table_name, namespace_id FROM system.tables")
                .await?;
            pb.finish_and_clear();
            resp
        } else {
            self.client
                .execute_query("SELECT table_name, namespace_id FROM system.tables")
                .await?
        };

        // Extract table names and namespace mapping from response
        let mut table_names = Vec::new();
        let mut ns_map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        if let Some(result) = response.results.first() {
            if let Some(rows) = &result.rows {
                for row in rows {
                    let name_opt = row.get("table_name").and_then(|v| v.as_str());
                    let ns_opt = row.get("namespace_id").and_then(|v| v.as_str());
                    if let Some(name) = name_opt {
                        table_names.push(name.to_string());
                        if let Some(ns) = ns_opt {
                            ns_map
                                .entry(ns.to_string())
                                .or_default()
                                .push(name.to_string());
                        }
                    }
                }
            }
        }

        completer.set_tables(table_names);
        for (ns, tables) in ns_map {
            completer.set_namespace_tables(ns, tables);
        }
        completer.clear_columns();

        if let Ok(column_response) = if self.animations {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                    .template("{spinner:.cyan} Fetching columns...")
                    .unwrap(),
            );
            pb.enable_steady_tick(Duration::from_millis(80));
            let resp = self
                .client
                .execute_query(
                    "SELECT table_name, column_name FROM information_schema.columns ORDER BY table_name, ordinal_position",
                )
                .await;
            pb.finish_and_clear();
            resp
        } else {
            self.client
                .execute_query(
                    "SELECT table_name, column_name FROM information_schema.columns ORDER BY table_name, ordinal_position",
                )
                .await
        } {
            if let Some(result) = column_response.results.first() {
                if let Some(rows) = &result.rows {
                    let mut column_map: HashMap<String, Vec<String>> = HashMap::new();

                    for row in rows {
                        if let (Some(table), Some(column)) = (
                            row.get("table_name").and_then(|v| v.as_str()),
                            row.get("column_name").and_then(|v| v.as_str()),
                        ) {
                            column_map
                                .entry(table.to_string())
                                .or_default()
                                .push(column.to_string());
                        }
                    }

                    for (table, columns) in column_map {
                        completer.set_columns(table, columns);
                    }
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
            Command::Health => match self.health_check().await {
                Ok(_) => {}
                Err(e) => eprintln!("Health check failed: {}", e),
            },
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
                // Show namespace along with table name and type
                self
                    .execute(
                        "SELECT namespace_id AS namespace, table_name, table_type FROM system.tables ORDER BY namespace_id, table_name",
                    )
                    .await?;
            }
            Command::RefreshTables => {
                // This is handled in run_interactive, shouldn't reach here
                println!("Table names refreshed");
            }
            Command::Describe(table) => {
                let query = format!(
                    "SELECT * FROM information_schema.columns WHERE table_name = '{}' ORDER BY ordinal_position",
                    table
                );
                self.execute(&query).await?;
            }
            Command::SetFormat(format) => match format.to_lowercase().as_str() {
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
            },
            Command::Subscribe(query) => {
                // Parse OPTIONS clause from SQL before creating config
                let (clean_sql, options) = Self::extract_subscribe_options(&query);
                let mut config = SubscriptionConfig::new(clean_sql);
                config.options = options;
                self.run_subscription(config).await?;
            }
            Command::Unsubscribe => {
                println!("No active subscription to cancel");
            }
            Command::ShowCredentials => {
                self.show_credentials();
            }
            Command::UpdateCredentials { username, password } => {
                self.update_credentials(username, password)?;
            }
            Command::DeleteCredentials => {
                self.delete_credentials()?;
            }
            Command::Info => {
                self.show_session_info();
            }
            Command::Stats => {
                // Show system statistics via system.stats virtual table
                // Keep it simple and readable for users
                self.execute("SELECT * FROM system.stats ORDER BY key")
                    .await?;
            }
            Command::Unknown(cmd) => {
                eprintln!("Unknown command: {}. Type \\help for help.", cmd);
            }
        }
        Ok(())
    }

    /// Extract OPTIONS clause from SUBSCRIBE SQL query.
    ///
    /// Parses `OPTIONS (last_rows=N)` from the SQL and returns cleaned SQL + options.
    /// If no OPTIONS found, returns original SQL with default options (last_rows=100).
    ///
    /// # Examples
    /// ```
    /// // Input:  "SELECT * FROM table OPTIONS (last_rows=20)"
    /// // Output: ("SELECT * FROM table", Some(SubscriptionOptions { last_rows: Some(20) }))
    /// ```
    fn extract_subscribe_options(sql: &str) -> (String, Option<SubscriptionOptions>) {
        let sql = sql.trim();
        let sql_upper = sql.to_uppercase();

        // Find OPTIONS keyword (case-insensitive)
        let options_idx = sql_upper
            .rfind(" OPTIONS ")
            .or_else(|| sql_upper.rfind(" OPTIONS("));

        let Some(idx) = options_idx else {
            // No OPTIONS found - return SQL as-is with default options
            return (
                sql.to_string(),
                Some(SubscriptionOptions {
                    last_rows: Some(100),
                }),
            );
        };

        // Split SQL at OPTIONS
        let clean_sql = sql[..idx].trim().to_string();
        let options_str = sql[idx + " OPTIONS".len()..].trim(); // " OPTIONS".len() == 8

        // Parse OPTIONS (last_rows=N)
        let options = Self::parse_subscribe_options(options_str);

        (clean_sql, options)
    }

    /// Parse OPTIONS clause value: (last_rows=N)
    fn parse_subscribe_options(options_str: &str) -> Option<SubscriptionOptions> {
        let options_str = options_str.trim();

        // Expected format: (last_rows=N) or ( last_rows = N )
        if !options_str.starts_with('(') || !options_str.ends_with(')') {
            eprintln!("Warning: Invalid OPTIONS format, using defaults");
            return Some(SubscriptionOptions {
                last_rows: Some(100),
            });
        }

        let inner = options_str[1..options_str.len() - 1].trim();

        // Parse last_rows=N
        if let Some(equals_idx) = inner.find('=') {
            let key = inner[..equals_idx].trim();
            let value = inner[equals_idx + 1..].trim();

            if key.to_lowercase() == "last_rows" {
                if let Ok(last_rows) = value.parse::<usize>() {
                    return Some(SubscriptionOptions {
                        last_rows: Some(last_rows),
                    });
                } else {
                    eprintln!(
                        "Warning: Invalid last_rows value '{}', using default 100",
                        value
                    );
                }
            } else {
                eprintln!("Warning: Unknown option '{}', ignoring", key);
            }
        }

        // Default to 100 rows if parsing failed
        Some(SubscriptionOptions {
            last_rows: Some(100),
        })
    }

    /// Run a WebSocket subscription
    ///
    /// **Implements T102**: WebSocket subscription display with timestamps and change indicators
    /// **Implements T103**: Ctrl+C handler for graceful subscription cancellation
    async fn run_subscription(&mut self, config: SubscriptionConfig) -> Result<()> {
        let sql_display = config.sql.clone();
        let ws_url_display = config.ws_url.clone();
        let requested_id = config.id.clone();

        // Suppress banner messages when running non-interactively (for test/automation)
        // Only print to stderr so stdout remains clean for data consumption
        if self.animations {
            eprintln!("Starting subscription for query: {}", sql_display);
            if let Some(ref ws_url) = ws_url_display {
                eprintln!("WebSocket endpoint: {}", ws_url);
            }
            if let Some(ref id) = requested_id {
                eprintln!("Requested subscription ID: {}", id);
            }
            eprintln!("Press Ctrl+C to unsubscribe and return to CLI\n");
        }

        let mut subscription = self.client.subscribe_with_config(config).await?;

        if self.animations {
            eprintln!(
                "Subscription established (ID: {})",
                subscription.subscription_id()
            );
        }

        // Set up Ctrl+C handler for graceful unsubscribe
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        loop {
            // Check if paused (T104)
            if self.subscription_paused {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            // Wait for either a subscription event or Ctrl+C
            tokio::select! {
                // Handle Ctrl+C
                _ = &mut ctrl_c => {
                    if self.color {
                        println!("\n\x1b[33m⚠ Unsubscribing...\x1b[0m");
                    } else {
                        println!("\n⚠ Unsubscribing...");
                    }
                    // Close subscription gracefully
                    if let Err(e) = subscription.close().await {
                        eprintln!("Warning: Failed to close subscription cleanly: {}", e);
                    }
                    if self.color {
                        println!("\x1b[32m✓ Unsubscribed\x1b[0m Back to CLI prompt");
                    } else {
                        println!("✓ Unsubscribed - Back to CLI prompt");
                    }
                    break;
                }

                // Handle subscription events
                event_result = subscription.next() => {
                    match event_result {
                        Some(Ok(event)) => {
                            // Check if it's an error event - if so, display and exit
                            if matches!(event, kalam_link::ChangeEvent::Error { .. }) {
                                self.display_change_event(&sql_display, &event);
                                println!("\nSubscription failed - returning to CLI prompt");
                                break;
                            }
                            self.display_change_event(&sql_display, &event);
                        }
                        Some(Err(e)) => {
                            eprintln!("Subscription error: {}", e);
                            break;
                        }
                        None => {
                            println!("Subscription ended by server");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Display a change event with formatting
    ///
    /// **Implements T102**: Change indicators (INSERT/UPDATE/DELETE)
    fn display_change_event(&self, subscription_sql: &str, event: &kalam_link::ChangeEvent) {
        use chrono::Local;
        let timestamp = Local::now().format("%H:%M:%S%.3f");

        match event {
            kalam_link::ChangeEvent::Ack {
                subscription_id,
                message: _,
            } => {
                let id_display = subscription_id.as_deref().unwrap_or("<pending-id>");
                if self.color {
                    println!(
                        "\x1b[36m[{}] ✓ SUBSCRIBED\x1b[0m [{}] Listening for changes...",
                        timestamp, id_display
                    );
                } else {
                    println!(
                        "[{}] ✓ SUBSCRIBED [{}] Listening for changes...",
                        timestamp, id_display
                    );
                }
            }
            kalam_link::ChangeEvent::InitialData {
                subscription_id,
                rows,
            } => {
                let count = rows.len();
                if self.color {
                    println!(
                        "\x1b[34m[{}] SNAPSHOT\x1b[0m [{}] {} rows for {}",
                        timestamp, subscription_id, count, subscription_sql
                    );
                } else {
                    println!(
                        "[{}] SNAPSHOT [{}] {} rows for {}",
                        timestamp, subscription_id, count, subscription_sql
                    );
                }

                let preview = rows.iter().take(5);
                for row in preview {
                    println!("    {}", Self::format_json(row));
                }
                if rows.len() > 5 {
                    println!("    ... ({} more rows)", rows.len() - 5);
                }
            }
            kalam_link::ChangeEvent::Insert {
                subscription_id,
                rows,
            } => {
                if rows.is_empty() {
                    if self.color {
                        println!(
                            "\x1b[32m[{}] INSERT\x1b[0m [{}] (no row payload)",
                            timestamp, subscription_id
                        );
                    } else {
                        println!(
                            "[{}] INSERT [{}] (no row payload)",
                            timestamp, subscription_id
                        );
                    }
                } else {
                    for row in rows {
                        let row_str = Self::format_json(row);
                        if self.color {
                            println!(
                                "\x1b[32m[{}] INSERT\x1b[0m [{}] {}",
                                timestamp, subscription_id, row_str
                            );
                        } else {
                            println!("[{}] INSERT [{}] {}", timestamp, subscription_id, row_str);
                        }
                    }
                }
            }
            kalam_link::ChangeEvent::Update {
                subscription_id,
                rows,
                old_rows,
            } => {
                let max_len = rows.len().max(old_rows.len());
                if max_len == 0 {
                    if self.color {
                        println!(
                            "\x1b[33m[{}] UPDATE\x1b[0m [{}] (no row payload)",
                            timestamp, subscription_id
                        );
                    } else {
                        println!(
                            "[{}] UPDATE [{}] (no row payload)",
                            timestamp, subscription_id
                        );
                    }
                } else {
                    for idx in 0..max_len {
                        let new_str = rows
                            .get(idx)
                            .map(Self::format_json)
                            .unwrap_or_else(|| "<missing>".to_string());
                        let old_str = old_rows
                            .get(idx)
                            .map(Self::format_json)
                            .unwrap_or_else(|| "<missing>".to_string());
                        if self.color {
                            println!(
                                "\x1b[33m[{}] UPDATE\x1b[0m [{}] {} ⇒ {}",
                                timestamp, subscription_id, old_str, new_str
                            );
                        } else {
                            println!(
                                "[{}] UPDATE [{}] {} => {}",
                                timestamp, subscription_id, old_str, new_str
                            );
                        }
                    }
                }
            }
            kalam_link::ChangeEvent::Delete {
                subscription_id,
                old_rows,
            } => {
                if old_rows.is_empty() {
                    if self.color {
                        println!(
                            "\x1b[31m[{}] DELETE\x1b[0m [{}] (no row payload)",
                            timestamp, subscription_id
                        );
                    } else {
                        println!(
                            "[{}] DELETE [{}] (no row payload)",
                            timestamp, subscription_id
                        );
                    }
                } else {
                    for row in old_rows {
                        let row_str = Self::format_json(row);
                        if self.color {
                            println!(
                                "\x1b[31m[{}] DELETE\x1b[0m [{}] {}",
                                timestamp, subscription_id, row_str
                            );
                        } else {
                            println!("[{}] DELETE [{}] {}", timestamp, subscription_id, row_str);
                        }
                    }
                }
            }
            kalam_link::ChangeEvent::Error {
                subscription_id,
                code,
                message,
            } => {
                if self.color {
                    eprintln!(
                        "\x1b[31m[{}] ERROR\x1b[0m [{}] {}: {}",
                        timestamp, subscription_id, code, message
                    );
                } else {
                    eprintln!(
                        "[{}] ERROR [{}] {}: {}",
                        timestamp, subscription_id, code, message
                    );
                }
            }
            kalam_link::ChangeEvent::Unknown { raw } => {
                // Log unknown payloads at debug level only - these are typically
                // system messages that don't need user attention
                if self.color {
                    eprintln!(
                        "\x1b[90m[{}] DEBUG: Unrecognized message type\x1b[0m",
                        timestamp
                    );
                } else {
                    eprintln!("[{}] DEBUG: Unrecognized message type", timestamp);
                }
                // Only show details in verbose mode
                #[cfg(debug_assertions)]
                eprintln!(
                    "  Payload: {}",
                    serde_json::to_string(raw).unwrap_or_default()
                );
            }
        }
    }

    /// Format a JSON value into a compact single-line string
    fn format_json(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => format!("\"{}\"", s),
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => serde_json::to_string(value).unwrap_or_else(|_| value.to_string()),
        }
    }

    /// Show help message (styled, sectioned)
    fn show_help(&self) {
        use colored::Colorize;

        println!();
        println!(
            "{}",
            "╔═══════════════════════════════════════════════════════════╗"
                .bright_blue()
                .bold()
        );
        println!(
            "{}{}{}",
            "║ ".bright_blue().bold(),
            "Commands & Shortcuts".white().bold(),
            "                                                 ║"
                .to_string()
                .bright_blue()
                .bold()
        );
        println!(
            "{}",
            "╠═══════════════════════════════════════════════════════════╣"
                .bright_blue()
                .bold()
        );

        // Basics
        println!("{}", "║  Basics".bright_blue().bold());
        println!("{}", "║    • Write SQL; end with ';' to run");
        println!(
            "{}{}",
            "║    • Autocomplete: keywords, namespaces, tables, columns  ",
            "(Tab)".dimmed()
        );
        println!("{}", "║    • Inline hints and SQL highlighting enabled");

        // Meta-commands (two columns)
        println!(
            "{}",
            "╠───────────────────────────────────────────────────────────╣"
                .bright_blue()
                .bold()
        );
        println!("{}", "║  Meta-Commands".bright_blue().bold());
        let left = [
            ("\\help, \\?", "Show this help"),
            ("\\quit, \\q", "Exit CLI"),
            ("\\info", "Session info"),
            ("\\connect <url>", "Connect to server"),
            ("\\config", "Show config"),
            ("\\format <type>", "table|json|csv"),
        ];
        let right = [
            ("\\dt", "List tables"),
            ("\\d <table>", "Describe table"),
            ("\\stats", "System stats"),
            ("\\health", "Health check"),
            ("\\refresh-tables", "Refresh autocomplete"),
            ("\\subscribe <SQL>", "Start live query"),
        ];
        for i in 0..left.len().max(right.len()) {
            let l = left
                .get(i)
                .map(|(a, b)| format!("{:<18} {:<18}", a.cyan(), b))
                .unwrap_or_else(|| "".into());
            let r = right
                .get(i)
                .map(|(a, b)| format!("{:<18} {:<18}", a.cyan(), b))
                .unwrap_or_else(|| "".into());
            println!("║  {}  {}{}", l, r, " ".repeat(7));
        }

        // Credentials
        println!(
            "{}",
            "╠───────────────────────────────────────────────────────────╣"
                .bright_blue()
                .bold()
        );
        println!("{}", "║  Credentials".bright_blue().bold());
        println!(
            "║    {:<24} Show stored credentials",
            "\\show-credentials".cyan()
        );
        println!(
            "║    {:<24} Update credentials",
            "\\update-credentials <u> <p>".cyan()
        );
        println!(
            "║    {:<24} Delete stored credentials",
            "\\delete-credentials".cyan()
        );

        // Tips & examples
        println!(
            "{}",
            "╠───────────────────────────────────────────────────────────╣"
                .bright_blue()
                .bold()
        );
        println!("{}", "║  Examples".bright_blue().bold());
        println!("║    {}", "SELECT * FROM system.tables LIMIT 5;".green());
        println!("║    {}", "SELECT name FROM system.namespaces;".green());
        println!("║    {}", "\\d system.jobs".green());

        println!(
            "{}",
            "╚═══════════════════════════════════════════════════════════╝"
                .bright_blue()
                .bold()
        );
        println!();
    }

    /// Show current session information
    ///
    /// Displays detailed information about the current CLI session
    fn show_session_info(&self) {
        use colored::Colorize;

        println!();
        println!(
            "{}",
            "═══════════════════════════════════════".cyan().bold()
        );
        println!("{}", "    Session Information".white().bold());
        println!(
            "{}",
            "═══════════════════════════════════════".cyan().bold()
        );
        println!();

        // Connection info
        println!("{}", "Connection:".yellow().bold());
        println!("  Server URL:     {}", self.server_url.green());
        println!("  Username:       {}", self.username.green());
        println!(
            "  Connected:      {}",
            if self.connected {
                "Yes".green()
            } else {
                "No".red()
            }
        );

        // Session timing
        let uptime = self.connected_at.elapsed();
        let hours = uptime.as_secs() / 3600;
        let minutes = (uptime.as_secs() % 3600) / 60;
        let seconds = uptime.as_secs() % 60;
        let uptime_str = if hours > 0 {
            format!("{}h {}m {}s", hours, minutes, seconds)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        };
        println!("  Session time:   {}", uptime_str.green());
        println!();

        // Server info
        println!("{}", "Server:".yellow().bold());
        if let Some(ref version) = self.server_version {
            println!("  Version:        {}", version.green());
        } else {
            println!("  Version:        {}", "Unknown".dimmed());
        }
        if let Some(ref api_version) = self.server_api_version {
            println!("  API Version:    {}", api_version.green());
        } else {
            println!("  API Version:    {}", "Unknown".dimmed());
        }
        if let Some(ref build_date) = self.server_build_date {
            println!("  Build Date:     {}", build_date.green());
        } else {
            println!("  Build Date:     {}", "Unknown".dimmed());
        }
        println!();

        // Client info
        println!("{}", "Client:".yellow().bold());
        println!("  CLI Version:    {}", env!("CARGO_PKG_VERSION").green());
        println!("  Build Date:     {}", env!("BUILD_DATE").green());
        println!("  Git Branch:     {}", env!("GIT_BRANCH").green());
        println!("  Git Commit:     {}", env!("GIT_COMMIT_HASH").green());
        println!();

        // Session statistics
        println!("{}", "Statistics:".yellow().bold());
        println!(
            "  Queries:        {}",
            self.queries_executed.to_string().green()
        );
        println!("  Format:         {}", format!("{:?}", self.format).green());
        println!(
            "  Colors:         {}",
            if self.color {
                "Enabled".green()
            } else {
                "Disabled".red()
            }
        );
        println!();

        // Instance info
        if let Some(ref instance) = self.instance {
            println!("{}", "Credentials:".yellow().bold());
            println!("  Instance:       {}", instance.green());
            println!();
        }

        println!(
            "{}",
            "═══════════════════════════════════════".cyan().bold()
        );
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

    /// Show stored credentials for current instance
    ///
    /// **Implements T121**: Display credentials command
    fn show_credentials(&self) {
        use colored::Colorize;
        use kalam_link::credentials::CredentialStore;

        match (&self.instance, &self.credential_store) {
            (Some(instance), Some(store)) => match store.get_credentials(instance) {
                Ok(Some(creds)) => {
                    println!("{}", "Stored Credentials".bold().cyan());
                    println!("  Instance: {}", creds.instance.green());
                    println!("  Username: {}", creds.username.green());
                    println!("  Password: {}", "****** (hidden)".dimmed());
                    if let Some(ref server_url) = creds.server_url {
                        println!("  Server URL: {}", server_url.green());
                    }
                    println!();
                    println!("{}", "Security Note:".yellow().bold());
                    println!(
                        "  Credentials are stored in: {}",
                        crate::credentials::FileCredentialStore::default_path()
                            .display()
                            .to_string()
                            .dimmed()
                    );
                    #[cfg(unix)]
                    println!(
                        "{}",
                        "  File permissions: 0600 (owner read/write only)".dimmed()
                    );
                }
                Ok(None) => {
                    println!("{}", "No credentials stored for this instance".yellow());
                    println!("Use \\update-credentials <username> <password> to store credentials");
                }
                Err(e) => {
                    eprintln!("{} {}", "Error loading credentials:".red(), e);
                }
            },
            (None, _) => {
                println!("{}", "Credential management not available".yellow());
                println!("Instance name not set for this session");
            }
            (_, None) => {
                println!("{}", "Credential store not available".yellow());
                println!("Credential storage was not initialized for this session");
            }
        }
    }

    /// Update credentials for current instance
    ///
    /// **Implements T122**: Update credentials command
    fn update_credentials(&mut self, username: String, password: String) -> Result<()> {
        use colored::Colorize;
        use kalam_link::credentials::{CredentialStore, Credentials};

        match (&self.instance, &mut self.credential_store) {
            (Some(instance), Some(store)) => {
                let creds = Credentials::with_server_url(
                    instance.clone(),
                    username.clone(),
                    password,
                    self.server_url.clone(),
                );

                store.set_credentials(&creds)?;

                println!("{}", "✓ Credentials updated successfully".green().bold());
                println!("  Instance: {}", instance.cyan());
                println!("  Username: {}", username.cyan());
                println!("  Server URL: {}", self.server_url.cyan());
                println!();
                println!("{}", "Security Reminder:".yellow().bold());
                println!(
                    "  Credentials are stored at: {}",
                    crate::credentials::FileCredentialStore::default_path()
                        .display()
                        .to_string()
                        .dimmed()
                );
                #[cfg(unix)]
                println!(
                    "{}",
                    "  File permissions: 0600 (owner read/write only)".dimmed()
                );

                Ok(())
            }
            (None, _) => Err(CLIError::ConfigurationError(
                "Instance name not set for this session".to_string(),
            )),
            (_, None) => Err(CLIError::ConfigurationError(
                "Credential store not initialized for this session".to_string(),
            )),
        }
    }

    /// Delete credentials for current instance
    ///
    /// **Implements T122**: Delete credentials functionality
    fn delete_credentials(&mut self) -> Result<()> {
        use colored::Colorize;
        use kalam_link::credentials::CredentialStore;

        match (&self.instance, &mut self.credential_store) {
            (Some(instance), Some(store)) => {
                store.delete_credentials(instance)?;

                println!("{}", "✓ Credentials deleted successfully".green().bold());
                println!("  Instance: {}", instance.cyan());
                println!();
                println!(
                    "You will need to provide authentication credentials for future connections."
                );

                Ok(())
            }
            (None, _) => Err(CLIError::ConfigurationError(
                "Instance name not set for this session".to_string(),
            )),
            (_, None) => Err(CLIError::ConfigurationError(
                "Credential store not initialized for this session".to_string(),
            )),
        }
    }

    /// Subscribe to a table or live query via command line
    ///
    /// This is similar to the interactive \subscribe command but designed for
    /// command-line usage where the subscription runs until interrupted.
    pub async fn subscribe(&mut self, query: &str) -> Result<()> {
        let config = SubscriptionConfig::new(query);
        self.run_subscription(config).await
    }

    /// Unsubscribe from active subscription via command line
    ///
    /// Since subscriptions run in a blocking loop, this method sends a signal
    /// to cancel the active subscription. In practice, this would need to be
    /// called from a different thread/context than the running subscription.
    pub async fn unsubscribe(&mut self, _subscription_id: &str) -> Result<()> {
        // For command-line usage, we can't easily interrupt a running subscription
        // from the same process. This would require a more complex signaling mechanism.
        // For now, inform the user how to cancel subscriptions.
        println!("To unsubscribe from an active subscription, use Ctrl+C in the terminal");
        println!("where the subscription is running, or kill the process.");
        Ok(())
    }

    /// List active subscriptions via command line
    ///
    /// Since subscriptions are managed per CLI session and run in blocking mode,
    /// this method informs about the current subscription state.
    pub async fn list_subscriptions(&mut self) -> Result<()> {
        // In the current architecture, subscriptions are managed per session
        // and there's no global subscription registry. We can only report
        // on the current session's subscription state.
        println!("Subscription management:");
        println!("  • Subscriptions run in blocking mode per CLI session");
        println!("  • Use Ctrl+C to cancel active subscriptions");
        println!("  • Each CLI instance can have at most one active subscription");
        println!("  • No persistent subscription registry is currently implemented");
        Ok(())
    }
}

type CharIter<'a> = std::iter::Peekable<std::str::Chars<'a>>;

struct SqlHighlighter {
    keywords: HashSet<String>,
    types: HashSet<String>,
    color_enabled: bool,
}

impl SqlHighlighter {
    fn new(color_enabled: bool) -> Self {
        let keywords = SQL_KEYWORDS
            .iter()
            .map(|kw| kw.to_ascii_uppercase())
            .collect::<HashSet<_>>();
        let types = SQL_TYPES
            .iter()
            .map(|kw| kw.to_ascii_uppercase())
            .collect::<HashSet<_>>();

        Self {
            keywords,
            types,
            color_enabled,
        }
    }

    fn color_enabled(&self) -> bool {
        self.color_enabled
    }

    fn highlight(&self, line: &str) -> Option<String> {
        if !self.color_enabled || line.trim().is_empty() {
            return None;
        }

        Some(self.highlight_line(line))
    }

    fn highlight_line(&self, line: &str) -> String {
        let mut result = String::with_capacity(line.len() * 2);
        let mut iter = line.chars().peekable();

        while let Some(ch) = iter.next() {
            if ch.is_whitespace() {
                result.push(ch);
                continue;
            }

            if ch == '-' {
                if let Some('-') = iter.peek().copied() {
                    result.push_str(&self.collect_comment(&mut iter));
                    break;
                } else {
                    result.push(ch);
                    continue;
                }
            }

            if ch == '\'' || ch == '"' {
                result.push_str(&self.collect_string(ch, &mut iter));
                continue;
            }

            if ch.is_ascii_digit() {
                result.push_str(&self.collect_number(ch, &mut iter));
                continue;
            }

            if ch.is_alphabetic() || ch == '_' {
                result.push_str(&self.collect_identifier(ch, &mut iter));
                continue;
            }

            result.push(ch);
        }

        result
    }

    fn collect_comment(&self, iter: &mut CharIter<'_>) -> String {
        let mut comment = String::from("--");
        iter.next();
        for ch in iter {
            comment.push(ch);
        }
        self.style_comment(&comment)
    }

    fn collect_string(&self, quote: char, iter: &mut CharIter<'_>) -> String {
        let mut literal = String::new();
        literal.push(quote);

        if quote == '\'' {
            while let Some(next) = iter.next() {
                literal.push(next);
                if next == quote {
                    if let Some(&dup) = iter.peek() {
                        if dup == quote {
                            literal.push(dup);
                            iter.next();
                            continue;
                        }
                    }
                    break;
                }
            }
        } else {
            let mut escaped = false;
            for next in iter.by_ref() {
                literal.push(next);
                if escaped {
                    escaped = false;
                    continue;
                }
                if next == '\\' {
                    escaped = true;
                    continue;
                }
                if next == quote {
                    break;
                }
            }
        }

        self.style_string(&literal)
    }

    fn collect_number(&self, first: char, iter: &mut CharIter<'_>) -> String {
        let mut number = String::new();
        number.push(first);

        while let Some(&next) = iter.peek() {
            if next.is_ascii_digit() || next == '_' || next == '.' {
                number.push(next);
                iter.next();
                continue;
            }

            if matches!(next, 'e' | 'E') {
                number.push(next);
                iter.next();
                if let Some(&sign) = iter.peek() {
                    if sign == '+' || sign == '-' {
                        number.push(sign);
                        iter.next();
                    }
                }
                continue;
            }

            break;
        }

        self.style_number(&number)
    }

    fn collect_identifier(&self, first: char, iter: &mut CharIter<'_>) -> String {
        let mut ident = String::new();
        ident.push(first);

        while let Some(&next) = iter.peek() {
            if next.is_ascii_alphanumeric() || next == '_' {
                ident.push(next);
                iter.next();
            } else {
                break;
            }
        }

        let upper = ident.to_ascii_uppercase();
        if self.types.contains(&upper) {
            self.style_type(&ident)
        } else if self.keywords.contains(&upper) {
            self.style_keyword(&ident)
        } else {
            self.style_identifier(&ident)
        }
    }

    fn style_keyword(&self, token: &str) -> String {
        token.blue().bold().to_string()
    }

    fn style_type(&self, token: &str) -> String {
        token.magenta().bold().to_string()
    }

    fn style_identifier(&self, token: &str) -> String {
        token.to_string()
    }

    fn style_number(&self, token: &str) -> String {
        token.yellow().to_string()
    }

    fn style_string(&self, token: &str) -> String {
        token.green().to_string()
    }

    fn style_comment(&self, token: &str) -> String {
        token.dimmed().to_string()
    }
}

/// Rustyline helper with autocomplete and highlighting
struct CLIHelper {
    completer: AutoCompleter,
    highlighter: SqlHighlighter,
}

impl CLIHelper {
    fn new(completer: AutoCompleter, color_enabled: bool) -> Self {
        Self {
            highlighter: SqlHighlighter::new(color_enabled),
            completer,
        }
    }
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

    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        self.completer.completion_hint(line, pos)
    }
}

impl Highlighter for CLIHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        if let Some(highlighted) = self.highlighter.highlight(line) {
            Cow::Owned(highlighted)
        } else {
            Cow::Borrowed(line)
        }
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        if self.highlighter.color_enabled() && !hint.is_empty() {
            Cow::Owned(hint.dimmed().to_string())
        } else {
            Cow::Borrowed(hint)
        }
    }
}

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
