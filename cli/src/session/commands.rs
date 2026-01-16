use super::{CLISession, OutputFormat};
use crate::error::Result;
use crate::parser::Command;
use colored::Colorize;
use kalam_link::SubscriptionConfig;

impl CLISession {
    /// Execute a parsed command
    ///
    /// **Implements T094**: Backslash command handling
    pub(super) async fn execute_command(&mut self, command: Command) -> Result<()> {
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
            Command::Flush => {
                println!("Storage flushing all tables in current namespace...");
                match self.execute("STORAGE FLUSH ALL").await {
                    Ok(_) => println!("Storage flush completed successfully"),
                    Err(e) => eprintln!("Storage flush failed: {}", e),
                }
            }
            Command::ClusterSnapshot => {
                println!("Triggering cluster snapshots...");
                match self.execute("CLUSTER SNAPSHOT").await {
                    Ok(_) => println!("Snapshot triggered"),
                    Err(e) => eprintln!("Snapshot failed: {}", e),
                }
            }
            Command::ClusterPurge { upto } => {
                println!("Purging cluster logs up to {}...", upto);
                match self.execute(&format!("CLUSTER PURGE --UPTO {}", upto)).await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster purge failed: {}", e),
                }
            }
            Command::ClusterTriggerElection => {
                println!("Triggering cluster election...");
                match self.execute("CLUSTER TRIGGER ELECTION").await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster trigger-election failed: {}", e),
                }
            }
            Command::ClusterTransferLeader { node_id } => {
                println!("Transferring cluster leadership to node {}...", node_id);
                match self.execute(&format!("CLUSTER TRANSFER-LEADER {}", node_id)).await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster transfer-leader failed: {}", e),
                }
            }
            Command::ClusterStepdown => {
                println!("Requesting cluster leader stepdown...");
                match self.execute("CLUSTER STEPDOWN").await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster stepdown failed: {}", e),
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
                match self.execute("SELECT * FROM system.cluster").await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster list failed: {}", e),
                }
            }
            Command::ClusterListGroups => {
                match self.execute("SELECT * FROM system.cluster_groups").await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster list groups failed: {}", e),
                }
            }
            Command::ClusterStatus => {
                match self.execute("SELECT * FROM system.cluster").await {
                    Ok(_) => {},
                    Err(e) => eprintln!("Cluster status failed: {}", e),
                }
            }
            Command::ClusterJoin(addr) => {
                println!("{}  CLUSTER JOIN is not implemented yet", "⚠️".yellow());
                println!("Would join node at: {}", addr);
                println!("\nTo add a node to the cluster, configure it in server.toml and restart.");
            }
            Command::ClusterLeave => {
                println!("{}  CLUSTER LEAVE is not implemented yet", "⚠️".yellow());
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
                self.execute(
                    "SELECT namespace_id AS namespace, table_name, table_type FROM system.tables ORDER BY namespace_id, table_name",
                )
                .await?;
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
                let (clean_sql, options) = Self::extract_subscribe_options(&query);
                let sub_id = format!(
                    "sub_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                );
                let mut config = SubscriptionConfig::new(sub_id, clean_sql);
                config.options = options;
                self.run_subscription(config).await?;
            }
            Command::Unsubscribe => {
                println!("No active subscription to cancel");
            }
            Command::RefreshTables => {
                // This is handled in run_interactive, shouldn't reach here
                println!("Table names refreshed");
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
                self.execute("SELECT metric_name, metric_value FROM system.stats ORDER BY metric_name")
                    .await?;
            }
            Command::Unknown(cmd) => {
                eprintln!("Unknown command: {}. Type \\help for help.", cmd);
            }
        }
        Ok(())
    }

    /// Show help message (styled, sectioned)
    fn show_help(&self) {
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
}
