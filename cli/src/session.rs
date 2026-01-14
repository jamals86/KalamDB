//! CLI session state management
//!
//! **Implements T084**: CLISession state with kalam-link client integration
//! **Implements T091-T093**: Interactive readline loop with command execution
//! **Implements T114a**: Loading indicator for long-running queries
//!
//! Manages the connection to KalamDB server and execution state throughout
//! the CLI session lifetime.

use crate::CLI_VERSION;
use clap::ValueEnum;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use kalam_link::{AuthProvider, ConnectionOptions, KalamLinkClient, KalamLinkTimeouts, SubscriptionConfig, SubscriptionOptions};
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

#[cfg(unix)]
use std::io::IsTerminal;

#[cfg(unix)]
use tokio::io::AsyncReadExt;

// Fallback system tables for autocomplete when the server does not return them
const SYSTEM_TABLES: &[&str] = &["users", "jobs", "namespaces", "storages", "live_queries", "tables", "audit_logs", "manifest", "stats", "settings", "server_logs", "cluster"];

#[cfg(unix)]
struct TerminalRawModeGuard {
    original: libc::termios,
}

#[cfg(unix)]
impl TerminalRawModeGuard {
    fn new() -> std::io::Result<Self> {
        unsafe {
            let fd = libc::STDIN_FILENO;
            let mut term: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut term) != 0 {
                return Err(std::io::Error::last_os_error());
            }

            let original = term;
            // We want to read Ctrl+C as raw byte (0x03) and allow single-byte reads,
            // but we must NOT disable output post-processing.
            //
            // `cfmakeraw` also disables output processing (OPOST), which stops the terminal
            // from translating `\n` into `\r\n`. That makes every subsequent line start at the
            // current column, producing the huge leading spaces seen during subscriptions.
            //
            // So we only adjust input-related flags and leave output flags untouched.
            term.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
            term.c_iflag &= !(libc::IXON | libc::ICRNL);
            term.c_cc[libc::VMIN] = 1;
            term.c_cc[libc::VTIME] = 0;

            if libc::tcsetattr(fd, libc::TCSANOW, &term) != 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(Self { original })
        }
    }
}

#[cfg(unix)]
impl Drop for TerminalRawModeGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
        }
    }
}

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

/// Cluster node information for CLI display
#[derive(Debug, Clone)]
struct ClusterNodeDisplay {
    node_id: u64,
    role: String,
    status: String,
    api_addr: String,
    is_self: bool,
    is_leader: bool,
}

/// Cluster information for CLI display
#[derive(Debug, Clone)]
struct ClusterInfoDisplay {
    is_cluster_mode: bool,
    cluster_name: String,
    current_node: Option<ClusterNodeDisplay>,
    nodes: Vec<ClusterNodeDisplay>,
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

    /// Cluster name from server (for prompt display)
    cluster_name: Option<String>,

    /// Credential store for managing saved credentials
    credential_store: Option<crate::credentials::FileCredentialStore>,

    /// Whether credentials were loaded from storage (vs. provided on command line)
    credentials_loaded: bool,

    /// Configured timeouts for operations
    #[allow(dead_code)] // Reserved for future use
    timeouts: KalamLinkTimeouts,
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
            server_url, auth, format, color, None, None, None, None, true, None, None, None, false,
        )
        .await
    }

    /// Create a new CLI session with AuthProvider, instance name, and credential store
    ///
    /// **Implements T121-T122**: CLI credential management commands
    #[allow(clippy::too_many_arguments)]
    pub async fn with_auth_and_instance(
        server_url: String,
        auth: AuthProvider,
        format: OutputFormat,
        color: bool,
        instance: Option<String>,
        credential_store: Option<crate::credentials::FileCredentialStore>,
        authenticated_username: Option<String>,
        loading_threshold_ms: Option<u64>,
        animations: bool,
        client_timeout: Option<Duration>,
        timeouts: Option<KalamLinkTimeouts>,
        connection_options: Option<ConnectionOptions>,
        credentials_loaded: bool,
    ) -> Result<Self> {
        // Build kalam-link client with authentication and timeouts
        let timeouts = timeouts.unwrap_or_default();
        let timeout = client_timeout.unwrap_or(timeouts.receive_timeout);
        
        // Build client with connection options if provided
        let mut builder = KalamLinkClient::builder()
            .base_url(&server_url)
            .timeout(timeout)
            .max_retries(3)
            .auth(auth.clone())
            .timeouts(timeouts.clone());
        
        if let Some(opts) = connection_options {
            builder = builder.connection_options(opts);
        }
        
        let client = builder.build()?;

        // Try to fetch server info from health check (sanitize empty strings to None)
        fn normalize_opt_string(s: String) -> Option<String> {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }

        let (server_version, server_api_version, server_build_date, connected) =
            match client.health_check().await {
                Ok(health) => (
                    normalize_opt_string(health.version),
                    normalize_opt_string(health.api_version),
                    health.build_date.and_then(|s| {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        }
                    }),
                    true,
                ),
                Err(_e) => (None, None, None, false),
            };

        // Use provided username or extract from auth provider
        let username = if let Some(name) = authenticated_username {
            name
        } else {
            match &auth {
                AuthProvider::BasicAuth(username, _) => username.clone(),
                AuthProvider::JwtToken(_) => "jwt-user".to_string(),
                AuthProvider::None => "anonymous".to_string(),
            }
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
            cluster_name: None, // Will be fetched lazily from system.cluster
            credential_store,
            credentials_loaded,
            timeouts,
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
        let result = self.client.execute_query(sql, None, None).await;

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
        if result.rows.as_ref().is_none_or(|r| r.is_empty()) {
            return Ok(None);
        }

        // Use row_as_map helper to get key-value access
        let row_map = match result.row_as_map(0) {
            Some(m) => m,
            None => return Ok(None),
        };
        let status = row_map.get("status").and_then(|v| v.as_str()).unwrap_or("");

        if status != "subscription_required" {
            return Ok(None);
        }

        let message = row_map
            .get("message")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let ws_url = row_map
            .get("ws_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let subscription_value = row_map.get("subscription").ok_or_else(|| {
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

        // Extract or generate subscription ID
        let sub_id = subscription_obj
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("sub_{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()));

        let mut config = SubscriptionConfig::new(sub_id, sql);

        if let Some(url) = ws_url {
            config.ws_url = Some(url);
        }

        if let Some(options_obj) = subscription_obj.get("options").and_then(|v| v.as_object()) {
            let options = SubscriptionOptions::default();
            let mut has_options = false;

            if let Some(_last_rows) = options_obj.get("last_rows").and_then(|v| v.as_u64()) {
                // Deprecated: batch streaming configured server-side
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
            } else if use_unicode {
                "○".yellow().bold().to_string()
            } else {
                "o".yellow().bold().to_string()
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

        // Use cluster_name for cluster mode, instance for saved connections, or "local" as fallback
        let display_name = self.cluster_name.as_deref()
            .or(self.instance.as_deref())
            .unwrap_or("local");
        
        let brand_with_profile = if use_colors_in_prompt {
            format!("{}{}", brand, format!("[{}]", display_name).dimmed())
        } else {
            format!("{}[{}]", brand, display_name)
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

        // Fetch cluster info to populate cluster_name and node address for prompt
        if let Some(cluster_info) = self.fetch_cluster_info().await {
            self.cluster_name = Some(cluster_info.cluster_name);
            // Update server_host to use the connected node's address if available
            if let Some(node) = &cluster_info.current_node {
                if !node.api_addr.is_empty() {
                    self.server_host = node.api_addr.clone();
                }
            }
        }

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
                CLI_VERSION,
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
        // Query namespaces first from system.namespaces
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
                .execute_query("SELECT name FROM system.namespaces ORDER BY name", None, None)
                .await;
            pb.finish_and_clear();
            resp
        } else {
            self.client
                .execute_query("SELECT name FROM system.namespaces ORDER BY name", None, None)
                .await
        };

        let mut namespaces: Vec<String> = Vec::new();
        if let Ok(ns_resp) = namespaces_res {
            if let Some(result) = ns_resp.results.first() {
                if let Some(rows) = &result.rows {
                    let name_idx = result.schema.iter().position(|f| f.name == "name");
                    for row in rows {
                        if let Some(idx) = name_idx {
                            if let Some(ns) = row.get(idx).and_then(|v| v.as_str()) {
                                namespaces.push(ns.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Query system.tables to get user table names and namespace mapping
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
                .execute_query("SELECT table_name, namespace_id FROM system.tables", None, None)
                .await?;
            pb.finish_and_clear();
            resp
        } else {
            self.client
                .execute_query("SELECT table_name, namespace_id FROM system.tables", None, None)
                .await?
        };

        // Extract table names and namespace mapping from response
        let mut table_names = Vec::new();
        let mut ns_map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        if let Some(result) = response.results.first() {
            if let Some(rows) = &result.rows {
                let table_name_idx = result.schema.iter().position(|f| f.name == "table_name");
                let ns_idx = result.schema.iter().position(|f| f.name == "namespace_id");

                for row in rows {
                    let name_opt = table_name_idx
                        .and_then(|idx| row.get(idx))
                        .and_then(|v| v.as_str());
                    let ns_opt = ns_idx
                        .and_then(|idx| row.get(idx))
                        .and_then(|v| v.as_str());
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

        // Also fetch system/information_schema tables from information_schema.tables for autocomplete
        let sys_tables_res = self
            .client
            .execute_query(
                "SELECT table_schema, table_name FROM information_schema.tables WHERE table_schema IN ('system', 'information_schema') ORDER BY table_schema, table_name",
                None,
                None,
            )
            .await;

        if let Ok(sys_resp) = sys_tables_res {
            if let Some(result) = sys_resp.results.first() {
                if let Some(rows) = &result.rows {
                    let table_name_idx = result.schema.iter().position(|f| f.name == "table_name");
                    let schema_idx = result.schema.iter().position(|f| f.name == "table_schema");

                    for row in rows {
                        let name_opt = table_name_idx
                            .and_then(|idx| row.get(idx))
                            .and_then(|v| v.as_str());
                        let schema_opt = schema_idx
                            .and_then(|idx| row.get(idx))
                            .and_then(|v| v.as_str());
                        if let (Some(name), Some(schema)) = (name_opt, schema_opt) {
                            // Add to table_names if not already present
                            if !table_names.contains(&name.to_string()) {
                                table_names.push(name.to_string());
                            }
                            // Add to ns_map for namespace.table autocomplete
                            ns_map
                                .entry(schema.to_string())
                                .or_default()
                                .push(name.to_string());
                            // Add namespace if not already present
                            if !namespaces.contains(&schema.to_string()) {
                                namespaces.push(schema.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Always ensure system namespace and known system tables are present for autocomplete
        {
            let sys_ns = "system".to_string();
            if !namespaces.contains(&sys_ns) {
                namespaces.push(sys_ns.clone());
            }

            for tbl in SYSTEM_TABLES {
                // Add to global table list
                if !table_names.contains(&tbl.to_string()) {
                    table_names.push(tbl.to_string());
                }
                // Add to namespace map
                ns_map.entry(sys_ns.clone()).or_default().push(tbl.to_string());
            }
        }

        // Sort namespaces
        namespaces.sort();
        namespaces.dedup();

        completer.set_namespaces(namespaces);
        completer.set_tables(table_names);
        for (ns, mut tables) in ns_map {
            tables.sort();
            tables.dedup();
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
                    None,
                    None,
                )
                .await;
            pb.finish_and_clear();
            resp
        } else {
            self.client
                .execute_query(
                    "SELECT table_name, column_name FROM information_schema.columns ORDER BY table_name, ordinal_position",
                    None,
                    None,
                )
                .await
        } {
            if let Some(result) = column_response.results.first() {
                if let Some(rows) = &result.rows {
                    let mut column_map: HashMap<String, Vec<String>> = HashMap::new();

                    // Find column indices in schema
                    let table_name_idx = result.schema.iter().position(|f| f.name == "table_name");
                    let column_name_idx = result.schema.iter().position(|f| f.name == "column_name");

                    for row in rows {
                        let table_opt = table_name_idx
                            .and_then(|idx| row.get(idx))
                            .and_then(|v| v.as_str());
                        let column_opt = column_name_idx
                            .and_then(|idx| row.get(idx))
                            .and_then(|v| v.as_str());
                        if let (Some(table), Some(column)) = (table_opt, column_opt) {
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
            Command::ClusterFlush => {
                println!("Flushing cluster logs to snapshots...");
                match self.execute("CLUSTER FLUSH").await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster flush failed: {}", e),
                }
            }
            Command::ClusterClear => {
                println!("Clearing old cluster snapshots...");
                match self.execute("CLUSTER CLEAR").await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster clear failed: {}", e),
                }
            }
            Command::ClusterList => {
                match self.execute("CLUSTER LIST").await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster list failed: {}", e),
                }
            }
            Command::ClusterJoin(addr) => {
                println!("{}", "⚠️  CLUSTER JOIN is not implemented yet".yellow());
                println!("Would join node at: {}", addr);
                println!("\nTo add a node to the cluster, configure it in server.toml and restart.");
            }
            Command::ClusterLeave => {
                println!("{}", "⚠️  CLUSTER LEAVE is not implemented yet".yellow());
                println!("\nTo remove this node from the cluster, gracefully shut down the server.");
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
                // Generate subscription ID
                let sub_id = format!("sub_{}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos());
                let mut config = SubscriptionConfig::new(sub_id, clean_sql);
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
                self.update_credentials(username, password).await?;
            }
            Command::DeleteCredentials => {
                self.delete_credentials()?;
            }
            Command::Info => {
                self.show_session_info().await;
            }
            Command::Stats => {
                // Show system statistics via system.stats virtual table
                // Keep it simple and readable for users
                self.execute("SELECT metric_name, metric_value FROM system.stats ORDER BY metric_name")
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
        // Trim and remove trailing semicolon if present
        let sql = sql.trim().trim_end_matches(';').trim();
        let sql_upper = sql.to_uppercase();

        // Find OPTIONS keyword (case-insensitive)
        let options_idx = sql_upper
            .rfind(" OPTIONS ")
            .or_else(|| sql_upper.rfind(" OPTIONS("));

        let Some(idx) = options_idx else {
            // No OPTIONS found - return SQL as-is with default options
            return (
                sql.to_string(),
                Some(SubscriptionOptions::default()),
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
            return Some(SubscriptionOptions::default());
        }

        let inner = options_str[1..options_str.len() - 1].trim();

        // Parse last_rows=N
        if let Some(equals_idx) = inner.find('=') {
            let key = inner[..equals_idx].trim();
            let value = inner[equals_idx + 1..].trim();

            if key.to_lowercase() == "last_rows" {
                if let Ok(last_rows) = value.parse::<u32>() {
                    return Some(SubscriptionOptions::new().with_last_rows(last_rows));
                } else {
                    eprintln!(
                        "Warning: Invalid last_rows value '{}', using default",
                        value
                    );
                }
            } else if key.to_lowercase() == "batch_size" {
                if let Ok(batch_size) = value.parse::<usize>() {
                    return Some(SubscriptionOptions::new().with_batch_size(batch_size));
                } else {
                    eprintln!(
                        "Warning: Invalid batch_size value '{}', using default",
                        value
                    );
                }
            } else {
                eprintln!("Warning: Unknown option '{}', ignoring", key);
            }
        }

        // Default if parsing failed
        Some(SubscriptionOptions::default())
    }

    /// Run a WebSocket subscription
    ///
    /// **Implements T102**: WebSocket subscription display with timestamps and change indicators
    /// **Implements T103**: Ctrl+C handler for graceful subscription cancellation
    #[cfg(unix)]
    async fn wait_for_exit_key_for_subscription() {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 1];

        loop {
            if stdin.read_exact(&mut buf).await.is_err() {
                break;
            }
            match buf[0] {
                3 | b'q' | b'Q' => break, // Ctrl+C or q
                _ => {}
            }
        }
    }

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
            eprintln!("Subscription ID: {}", requested_id);
            eprintln!("Press Ctrl+C (or 'q') to unsubscribe and return to CLI\n");
        }

        let mut subscription = self.client.subscribe_with_config(config).await?;

        if self.animations {
            eprintln!(
                "Subscription established (ID: {})",
                subscription.subscription_id()
            );
        }

        // On unix TTYs, SIGINT can be intercepted by the readline layer.
        // Switch stdin to raw mode and watch for Ctrl+C bytes (0x03) / 'q' for a reliable exit.
        #[cfg(unix)]
        if std::io::stdin().is_terminal() {
            if let Ok(_raw_guard) = TerminalRawModeGuard::new() {
                let mut exit_key = Box::pin(Self::wait_for_exit_key_for_subscription());

                loop {
                    if self.subscription_paused {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }

                    tokio::select! {
                        _ = exit_key.as_mut() => {
                            if self.color {
                                println!("\n\x1b[33m⚠ Unsubscribing...\x1b[0m");
                            } else {
                                println!("\n⚠ Unsubscribing...");
                            }

                            // Close, but don't hang forever.
                            let close_res = tokio::time::timeout(Duration::from_secs(2), subscription.close()).await;
                            if let Err(_) = close_res {
                                eprintln!("Warning: Timed out while closing subscription; exiting anyway");
                            } else if let Ok(Err(e)) = close_res {
                                eprintln!("Warning: Failed to close subscription cleanly: {}", e);
                            }

                            if self.color {
                                println!("\x1b[32m✓ Unsubscribed\x1b[0m Back to CLI prompt");
                            } else {
                                println!("✓ Unsubscribed - Back to CLI prompt");
                            }
                            break;
                        }

                        event_result = subscription.next() => {
                            match event_result {
                                Some(Ok(event)) => {
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

                return Ok(());
            }
        }

        // Fallback: SIGINT-based cancellation.
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
                    // Close subscription gracefully, but don't hang forever.
                    let close_res = tokio::time::timeout(Duration::from_secs(2), subscription.close()).await;
                    if let Err(_) = close_res {
                        eprintln!("Warning: Timed out while closing subscription; exiting anyway");
                    } else if let Ok(Err(e)) = close_res {
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

    /// Run a WebSocket subscription with an optional timeout
    ///
    /// If timeout is Some, the subscription will exit after the specified duration
    /// once initial data has been received. This is useful for testing.
    async fn run_subscription_with_timeout(&mut self, config: SubscriptionConfig, timeout: Option<std::time::Duration>) -> Result<()> {
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
            eprintln!("Subscription ID: {}", requested_id);
            if let Some(timeout) = timeout {
                eprintln!("Timeout: {:?}", timeout);
            } else {
                eprintln!("Press Ctrl+C (or 'q') to unsubscribe and return to CLI");
            }
            eprintln!();
        }

        let mut subscription = self.client.subscribe_with_config(config).await?;

        if self.animations {
            eprintln!(
                "Subscription established (ID: {})",
                subscription.subscription_id()
            );
        }

        // Unix TTY path: raw-mode key cancel (Ctrl+C byte / 'q')
        #[cfg(unix)]
        if std::io::stdin().is_terminal() {
            if let Ok(_raw_guard) = TerminalRawModeGuard::new() {
                let mut exit_key = Box::pin(Self::wait_for_exit_key_for_subscription());

                // Track when initial data is complete and when timeout should fire
                let mut initial_data_complete = false;
                let timeout_deadline = timeout.map(|d| tokio::time::Instant::now() + d);

                loop {
                    if initial_data_complete {
                        if let Some(deadline) = timeout_deadline {
                            if tokio::time::Instant::now() >= deadline {
                                if self.animations {
                                    eprintln!("\n⏱ Subscription timeout reached");
                                }
                                let close_res = tokio::time::timeout(Duration::from_secs(2), subscription.close()).await;
                                if let Err(_) = close_res {
                                    eprintln!("Warning: Timed out while closing subscription; exiting anyway");
                                } else if let Ok(Err(e)) = close_res {
                                    eprintln!("Warning: Failed to close subscription cleanly: {}", e);
                                }
                                break;
                            }
                        }
                    }

                    if self.subscription_paused {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }

                    let poll_timeout = if timeout.is_some() {
                        tokio::time::Duration::from_millis(100)
                    } else {
                        tokio::time::Duration::from_secs(3600)
                    };

                    tokio::select! {
                        _ = exit_key.as_mut() => {
                            if self.color {
                                println!("\n\x1b[33m⚠ Unsubscribing...\x1b[0m");
                            } else {
                                println!("\n⚠ Unsubscribing...");
                            }
                            let close_res = tokio::time::timeout(Duration::from_secs(2), subscription.close()).await;
                            if let Err(_) = close_res {
                                eprintln!("Warning: Timed out while closing subscription; exiting anyway");
                            } else if let Ok(Err(e)) = close_res {
                                eprintln!("Warning: Failed to close subscription cleanly: {}", e);
                            }
                            if self.color {
                                println!("\x1b[32m✓ Unsubscribed\x1b[0m Back to CLI prompt");
                            } else {
                                println!("✓ Unsubscribed - Back to CLI prompt");
                            }
                            break;
                        }

                        _ = tokio::time::sleep(poll_timeout) => {
                            continue;
                        }

                        event_result = subscription.next() => {
                            match event_result {
                                Some(Ok(event)) => {
                                    if matches!(event, kalam_link::ChangeEvent::Error { .. }) {
                                        self.display_change_event(&sql_display, &event);
                                        println!("\nSubscription failed - returning to CLI prompt");
                                        break;
                                    }

                                    match &event {
                                        kalam_link::ChangeEvent::InitialDataBatch { batch_control, .. } => {
                                            if !batch_control.has_more {
                                                initial_data_complete = true;
                                            }
                                        }
                                        kalam_link::ChangeEvent::Ack { batch_control, .. } => {
                                            if !batch_control.has_more {
                                                initial_data_complete = true;
                                            }
                                        }
                                        _ => {}
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

                return Ok(());
            }
        }

        // Fallback: SIGINT-based cancellation.
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        // Track when initial data is complete and when timeout should fire
        let mut initial_data_complete = false;
        let timeout_deadline = timeout.map(|d| tokio::time::Instant::now() + d);

        loop {
            // Check timeout after initial data is received
            if initial_data_complete {
                if let Some(deadline) = timeout_deadline {
                    if tokio::time::Instant::now() >= deadline {
                        if self.animations {
                            eprintln!("\n⏱ Subscription timeout reached");
                        }
                        // Close subscription gracefully, but don't hang forever.
                        let close_res = tokio::time::timeout(Duration::from_secs(2), subscription.close()).await;
                        if let Err(_) = close_res {
                            eprintln!("Warning: Timed out while closing subscription; exiting anyway");
                        } else if let Ok(Err(e)) = close_res {
                            eprintln!("Warning: Failed to close subscription cleanly: {}", e);
                        }
                        break;
                    }
                }
            }

            // Check if paused (T104)
            if self.subscription_paused {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            // Create a timeout for the select (if we have a timeout configured)
            let poll_timeout = if timeout.is_some() {
                tokio::time::Duration::from_millis(100)
            } else {
                tokio::time::Duration::from_secs(3600) // Effectively infinite
            };

            // Wait for either a subscription event, Ctrl+C, or poll timeout
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

                // Poll timeout - just continue the loop to check deadline
                _ = tokio::time::sleep(poll_timeout) => {
                    continue;
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
                            
                            // Check if initial data is complete (batch with has_more=false)
                            match &event {
                                kalam_link::ChangeEvent::InitialDataBatch { batch_control, .. } => {
                                    if !batch_control.has_more {
                                        initial_data_complete = true;
                                    }
                                }
                                kalam_link::ChangeEvent::Ack { batch_control, .. } => {
                                    if !batch_control.has_more {
                                        initial_data_complete = true;
                                    }
                                }
                                _ => {}
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
    fn display_change_event(&self, _subscription_sql: &str, event: &kalam_link::ChangeEvent) {
        use chrono::Local;
        let timestamp = Local::now().format("%H:%M:%S%.3f");

        match event {
            kalam_link::ChangeEvent::Ack {
                subscription_id,
                total_rows,
                batch_control,
                schema,
            } => {
                if self.color {
                    println!(
                        "\x1b[36m[{}] ✓ SUBSCRIBED\x1b[0m [{}] {} total rows, batch {} {}, {} columns",
                        timestamp,
                        subscription_id,
                        total_rows,
                        batch_control.batch_num + 1,
                        if batch_control.has_more {
                            "(loading...)"
                        } else {
                            "(ready)"
                        },
                        schema.len()
                    );
                } else {
                    println!(
                        "[{}] ✓ SUBSCRIBED [{}] {} total rows, batch {} {}, {} columns",
                        timestamp,
                        subscription_id,
                        total_rows,
                        batch_control.batch_num + 1,
                        if batch_control.has_more {
                            "(loading...)"
                        } else {
                            "(ready)"
                        },
                        schema.len()
                    );
                }
            }
            kalam_link::ChangeEvent::InitialDataBatch {
                subscription_id,
                rows,
                batch_control,
            } => {
                let count = rows.len();
                if self.color {
                    println!(
                        "\x1b[34m[{}] BATCH {}\x1b[0m [{}] {} rows {}",
                        timestamp,
                        batch_control.batch_num + 1,
                        subscription_id,
                        count,
                        if batch_control.has_more {
                            "(more pending)"
                        } else {
                            "(complete)"
                        }
                    );
                } else {
                    println!(
                        "[{}] BATCH {} [{}] {} rows {}",
                        timestamp,
                        batch_control.batch_num + 1,
                        subscription_id,
                        count,
                        if batch_control.has_more {
                            "(more pending)"
                        } else {
                            "(complete)"
                        }
                    );
                }

                // Display rows in the same format as snapshots
                for row in rows {
                    let formatted = serde_json::to_string_pretty(&row).unwrap_or_default();
                    if self.color {
                        println!("  \x1b[90m{}\x1b[0m", formatted);
                    } else {
                        println!("  {}", formatted);
                    }
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
                    serde_json::to_string(&raw).unwrap_or_default()
                );
                #[cfg(not(debug_assertions))]
                let _ = raw; // Suppress unused warning in release builds
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
        println!("║    • Write SQL; end with ';' to run");
        println!(
            "║    • Autocomplete: keywords, namespaces, tables, columns  {}",
            "(Tab)".dimmed()
        );
        println!("║    • Inline hints and SQL highlighting enabled");

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

    /// Fetch cluster information from system.cluster
    async fn fetch_cluster_info(&self) -> Option<ClusterInfoDisplay> {
        // Query the system.cluster table (cluster_id is now the first column)
        let result = self
            .client
            .execute_query(
                "SELECT cluster_id, node_id, role, status, api_addr, is_self, is_leader FROM system.cluster",
                None,
                None,
            )
            .await;

        match result {
            Ok(response) => {
                let mut nodes = Vec::new();
                let mut current_node = None;
                let mut is_cluster_mode = false;
                let mut cluster_name = String::new();

                // Get the first result set
                if let Some(query_result) = response.results.first() {
                    if let Some(rows) = &query_result.rows {
                        for row in rows {
                            // row is a Vec<JsonValue> with fields in order: cluster_id, node_id, role, status, api_addr, is_self, is_leader
                            if row.len() >= 7 {
                                // Extract cluster_id from first row only
                                if cluster_name.is_empty() {
                                    cluster_name = row[0].as_str().unwrap_or("standalone").to_string();
                                }
                                let node_id = row[1].as_u64().unwrap_or(0);
                                let role = row[2].as_str().unwrap_or("unknown").to_string();
                                let status = row[3].as_str().unwrap_or("unknown").to_string();
                                let api_addr = row[4].as_str().unwrap_or("").to_string();
                                let is_self = row[5].as_bool().unwrap_or(false);
                                let is_leader = row[6].as_bool().unwrap_or(false);

                                // Check if this looks like cluster mode (role is leader/follower)
                                if role == "leader" || role == "follower" {
                                    is_cluster_mode = true;
                                }

                                let node = ClusterNodeDisplay {
                                    node_id,
                                    role,
                                    status,
                                    api_addr,
                                    is_self,
                                    is_leader,
                                };

                                if is_self {
                                    current_node = Some(node.clone());
                                }
                                nodes.push(node);
                            }
                        }
                    }
                }

                // If we only have one node and it's standalone, we're not in cluster mode
                if nodes.len() <= 1 && nodes.iter().any(|n| n.role == "standalone") {
                    is_cluster_mode = false;
                }

                Some(ClusterInfoDisplay {
                    is_cluster_mode,
                    cluster_name,
                    current_node,
                    nodes,
                })
            }
            Err(_) => None,
        }
    }

    /// Show current session information
    ///
    /// Displays detailed information about the current CLI session
    async fn show_session_info(&mut self) {
        use colored::Colorize;

        // Fetch cluster info from system.cluster_nodes
        let cluster_info = self.fetch_cluster_info().await;

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

        // Cluster info (if server provided it)
        println!("{}", "Cluster:".yellow().bold());
        if let Some(ref info) = cluster_info {
            println!(
                "  Mode:           {}",
                if info.is_cluster_mode {
                    "Cluster".green()
                } else {
                    "Standalone".dimmed()
                }
            );
            if info.is_cluster_mode {
                // Show current node (the one we're connected to)
                if let Some(ref current_node) = info.current_node {
                    println!("  Connected Node: {}", format!("Node {}", current_node.node_id).green());
                    println!("  Node Role:      {}", current_node.role.green());
                    println!("  Node API:       {}", current_node.api_addr.green());
                }
                // Show all cluster nodes
                println!();
                println!("  {}", "Cluster Nodes:".yellow());
                for node in &info.nodes {
                    let self_marker = if node.is_self { " (connected)" } else { "" };
                    let leader_marker = if node.is_leader { " [LEADER]" } else { "" };
                    println!(
                        "    Node {}: {} | {} | {}{}{}",
                        node.node_id,
                        node.role,
                        node.status,
                        node.api_addr,
                        leader_marker.yellow(),
                        self_marker.cyan()
                    );
                }
            }
        } else {
            println!(
                "  Mode:           {}",
                "Standalone".dimmed()
            );
            println!(
                "  {}",
                "(Could not fetch cluster info)".dimmed()
            );
        }
        println!(
            "  {}",
            "Use 'SELECT * FROM system.cluster' for full details"
                .dimmed()
        );
        println!();

        // Client info
        println!("{}", "Client:".yellow().bold());
        println!("  CLI Version:    {}", CLI_VERSION.green());
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

        // Credentials info
        println!("{}", "Credentials:".yellow().bold());
        if let Some(ref instance) = self.instance {
            println!("  Instance:       {}", instance.green());
        }
        println!(
            "  Loaded:         {}",
            if self.credentials_loaded {
                "Yes (from stored credentials)".green()
            } else {
                "No (provided via CLI args)".dimmed()
            }
        );
        if self.credential_store.is_some() {
            println!(
                "  Storage:        {}",
                crate::credentials::FileCredentialStore::default_path()
                    .display()
                    .to_string()
                    .dimmed()
            );
        }
        println!();

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
                    if let Some(ref username) = creds.username {
                        println!("  Username: {}", username.green());
                    }
                    // Show truncated JWT token
                    let token_preview = if creds.jwt_token.len() > 30 {
                        format!("{}...", &creds.jwt_token[..30])
                    } else {
                        creds.jwt_token.clone()
                    };
                    println!("  JWT Token: {}", token_preview.dimmed());
                    if let Some(ref expires) = creds.expires_at {
                        let expired_marker = if creds.is_expired() { " (EXPIRED)".red().to_string() } else { "".to_string() };
                        println!("  Expires: {}{}", expires.green(), expired_marker);
                    }
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
                    println!("Use --username and --password to login and store credentials");
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
    /// Performs login to get JWT token and stores it
    async fn update_credentials(&mut self, username: String, password: String) -> Result<()> {
        use colored::Colorize;
        use kalam_link::credentials::{CredentialStore, Credentials};

        match (&self.instance, &mut self.credential_store) {
            (Some(instance), Some(store)) => {
                // Perform login to get JWT token
                println!("{}", "Logging in...".dimmed());
                
                let login_result = self.client.login(&username, &password).await;
                
                match login_result {
                    Ok(login_response) => {
                        let creds = Credentials::with_details(
                            instance.clone(),
                            login_response.access_token,
                            login_response.user.username.clone(),
                            login_response.expires_at.clone(),
                            Some(self.server_url.clone()),
                        );

                        store.set_credentials(&creds)?;

                        println!("{}", "✓ Credentials updated successfully".green().bold());
                        println!("  Instance: {}", instance.cyan());
                        println!("  Username: {}", login_response.user.username.cyan());
                        println!("  Expires: {}", login_response.expires_at.cyan());
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
                    Err(e) => {
                        Err(CLIError::ConfigurationError(format!("Login failed: {}", e)))
                    }
                }
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
        self.subscribe_with_timeout(query, None).await
    }

    /// Subscribe to a table or live query with an optional timeout
    ///
    /// If timeout is Some, the subscription will exit after the specified duration
    /// once initial data has been received. This is useful for testing.
    pub async fn subscribe_with_timeout(&mut self, query: &str, timeout: Option<std::time::Duration>) -> Result<()> {
        // Generate subscription ID
        let sub_id = format!("sub_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos());
        let config = SubscriptionConfig::new(sub_id, query);
        self.run_subscription_with_timeout(config, timeout).await
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
                    // Comment consumes rest of line, so we're done with this line
                    return result;
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

    #[test]
    fn test_extract_subscribe_options_with_semicolon() {
        // Test that semicolon is properly trimmed from subscription queries
        let (sql, _) = CLISession::extract_subscribe_options("SELECT * FROM table;");
        assert_eq!(sql, "SELECT * FROM table");

        let (sql, _) = CLISession::extract_subscribe_options("SELECT * FROM table ;");
        assert_eq!(sql, "SELECT * FROM table");

        let (sql, _) = CLISession::extract_subscribe_options("SELECT * FROM table");
        assert_eq!(sql, "SELECT * FROM table");
    }

    #[test]
    fn test_extract_subscribe_options_with_options_and_semicolon() {
        // Test OPTIONS parsing with semicolon
        let (sql, options) = CLISession::extract_subscribe_options(
            "SELECT * FROM table OPTIONS (last_rows=50);"
        );
        assert_eq!(sql, "SELECT * FROM table");
        assert!(options.is_some());
    }
}
