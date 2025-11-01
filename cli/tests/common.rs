//! Common utilities and helpers for CLI integration tests
//!
//! This module provides shared functionality used across all CLI test modules,
//! including server connectivity checks, CLI command helpers, test data setup,
//! and specialized tools like subscription message listeners.

use assert_cmd::Command;
use std::time::Duration;
use std::process::{Child, Stdio};
use std::io::{BufRead, BufReader};
use std::sync::mpsc as std_mpsc;
use std::thread;

/// Test configuration constants
pub const SERVER_URL: &str = "http://localhost:8080";
pub const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Helper to check if server is running via CLI
pub fn is_server_running() -> bool {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("SELECT 1")
        .timeout(Duration::from_secs(2));

    cmd.output().map(|output| output.status.success()).unwrap_or(false)
}

/// Helper to create a CLI command with default test settings
pub fn create_cli_command() -> Command {
    let cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd
}

/// Helper to execute SQL via CLI command
pub fn execute_sql_via_cli(sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(sql)
        .timeout(TEST_TIMEOUT);

    let output = cmd.output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!("CLI command failed: {}", String::from_utf8_lossy(&output.stderr)).into())
    }
}

/// Helper to execute SQL via CLI with authentication
pub fn execute_sql_via_cli_as(username: &str, password: &str, sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--username")
        .arg(username)
        .arg("--password")
        .arg(password)
        .arg("--command")
        .arg(sql)
        .timeout(TEST_TIMEOUT);

    let output = cmd.output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!("CLI command failed: {}", String::from_utf8_lossy(&output.stderr)).into())
    }
}

/// Helper to execute SQL as root user via CLI (empty password for localhost)
pub fn execute_sql_as_root_via_cli(sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    execute_sql_via_cli_as("root", "", sql)
}

/// Helper to generate unique namespace name
pub fn generate_unique_namespace(base_name: &str) -> String {
    format!(
        "{}_{}",
        base_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

/// Helper to generate unique table name
pub fn generate_unique_table(base_name: &str) -> String {
    format!(
        "{}_{}",
        base_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

/// Helper to wait for a condition with timeout
pub async fn wait_for_condition<F, Fut>(
    mut condition: F,
    timeout: Duration,
    check_interval: Duration,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        tokio::time::sleep(check_interval).await;
    }

    Err("Condition not met within timeout".into())
}

/// Subscription listener for testing real-time events
pub struct SubscriptionListener {
    child: Child,
    stdout_reader: BufReader<std::process::ChildStdout>,
    event_sender: std_mpsc::Sender<String>,
    _event_receiver: std_mpsc::Receiver<String>,
}

impl SubscriptionListener {
    /// Start a subscription listener for a given query
    pub fn start(query: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let (event_sender, event_receiver) = std_mpsc::channel();

        let mut cmd = create_cli_command();
        cmd.arg("-u")
            .arg(SERVER_URL)
            .arg("--subscribe")
            .arg(query)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stdout_reader = BufReader::new(stdout);

        Ok(Self {
            child,
            stdout_reader,
            event_sender,
            _event_receiver: event_receiver,
        })
    }

    /// Wait for a specific event pattern
    pub fn wait_for_event(&mut self, pattern: &str, timeout: Duration) -> Result<String, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        let mut buffer = String::new();

        while start.elapsed() < timeout {
            let mut line = String::new();
            match self.stdout_reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    buffer.push_str(&line);
                    if line.contains(pattern) {
                        return Ok(line.trim().to_string());
                    }
                }
                Err(_) => break,
            }
        }

        Err(format!("Event pattern '{}' not found within timeout", pattern).into())
    }

    /// Stop the subscription listener
    pub fn stop(mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.child.kill()?;
        self.child.wait()?;
        Ok(())
    }
}

/// Helper to start a subscription listener in a background thread
pub fn start_subscription_listener(query: &str) -> Result<std_mpsc::Receiver<String>, Box<dyn std::error::Error>> {
    let (event_sender, event_receiver) = std_mpsc::channel();

    thread::spawn(move || {
        let mut listener = match SubscriptionListener::start(query) {
            Ok(l) => l,
            Err(e) => {
                let _ = event_sender.send(format!("ERROR: {}", e));
                return;
            }
        };

        loop {
            let mut line = String::new();
            match listener.stdout_reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let _ = event_sender.send(line.trim().to_string());
                }
                Err(_) => break,
            }
        }
    });

    Ok(event_receiver)
}

/// Helper to execute SQL with authentication
pub async fn execute_sql_as(
    username: &str,
    password: &str,
    sql: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .basic_auth(username, Some(password))
        .json(&json!({ "sql": sql }))
        .send()
        .await?;

    let body = response.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&body)?;
    Ok(parsed)
}

/// Helper to execute SQL as root user (empty password for localhost)
pub async fn execute_sql_as_root(sql: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    execute_sql_as("root", "", sql).await
}

/// Helper to setup test namespace and table with unique name per test
pub async fn setup_test_data(test_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Use test-specific table name to avoid conflicts
    let table_name = format!("messages_{}", test_name);
    let namespace = "test_cli";

    // Longer delay to avoid race conditions between tests
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Try to drop table first if it exists
    let drop_sql = format!("DROP TABLE IF EXISTS {}.{}", namespace, table_name);
    let _ = execute_sql(&drop_sql).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Create namespace (don't drop it - let it persist across tests)
    match execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await {
        Ok(_) => {
            // Namespace created successfully
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        Err(e) if e.to_string().contains("already exists") => {
            // Namespace exists, that's ok
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        Err(e) => return Err(e),
    }

    // Create test table using USER TABLE
    let create_sql = format!(
        r#"CREATE USER TABLE {}.{} (
            id INT AUTO_INCREMENT,
            content VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
        namespace, table_name
    );

    match execute_sql(&create_sql).await {
        Ok(_) => {}
        Err(e) if e.to_string().contains("already exists") => {
            // Table exists, drop and recreate to ensure clean state
            execute_sql(&drop_sql).await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            execute_sql(&create_sql).await?;
        }
        Err(e) => return Err(e),
    }

    // Small delay after table creation
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(format!("{}.{}", namespace, table_name))
}

/// Helper to cleanup test data
pub async fn cleanup_test_data(table_full_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Delete the table
    let drop_sql = format!("DROP TABLE IF EXISTS {}", table_full_name);
    let _ = execute_sql(&drop_sql).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    Ok(())
}

/// Helper to create a CLI command with default test settings
pub fn create_cli_command() -> Command {
    let cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd
}

/// Subscription message listener for testing WebSocket subscriptions
pub struct SubscriptionListener {
    receiver: mpsc::Receiver<serde_json::Value>,
    _handle: tokio::task::JoinHandle<()>,
}

impl SubscriptionListener {
    /// Create a new subscription listener for a given subscription URL
    pub async fn new(subscription_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel(100);

        let url = subscription_url.to_string();
        let handle = tokio::spawn(async move {
            if let Ok((ws_stream, _)) = connect_async(&url).await {
                let (mut write, mut read) = ws_stream.split();

                while let Some(message) = read.next().await {
                    match message {
                        Ok(msg) => {
                            if let Ok(text) = msg.to_text() {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
                                    let _ = tx.send(json).await;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        });

        Ok(Self {
            receiver: rx,
            _handle: handle,
        })
    }

    /// Receive the next subscription message with timeout
    pub async fn receive_message(&mut self, timeout: Duration) -> Option<serde_json::Value> {
        tokio::time::timeout(timeout, self.receiver.recv()).await.ok().flatten()
    }

    /// Receive all available messages up to a maximum count
    pub async fn receive_all_messages(&mut self, max_count: usize, timeout: Duration) -> Vec<serde_json::Value> {
        let mut messages = Vec::new();
        let start_time = std::time::Instant::now();

        while messages.len() < max_count && start_time.elapsed() < timeout {
            if let Some(msg) = self.receive_message(Duration::from_millis(100)).await {
                messages.push(msg);
            } else {
                break;
            }
        }

        messages
    }
}

/// Helper to create a unique test namespace name
pub fn generate_unique_namespace(base_name: &str) -> String {
    format!(
        "{}_{}",
        base_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

/// Helper to create a unique test table name
pub fn generate_unique_table(base_name: &str) -> String {
    format!(
        "{}_{}",
        base_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

/// Helper to wait for a condition with timeout
pub async fn wait_for_condition<F, Fut>(
    mut condition: F,
    timeout: Duration,
    check_interval: Duration,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        tokio::time::sleep(check_interval).await;
    }

    Err("Condition not met within timeout".into())
}

/// Helper to create temporary SQL file for batch execution tests
pub fn create_temp_sql_file(content: &str) -> Result<tempfile::NamedTempFile, Box<dyn std::error::Error>> {
    let mut temp_file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut temp_file, content.as_bytes())?;
    Ok(temp_file)
}

/// Helper to create temporary config file for config tests
pub fn create_temp_config_file(content: &str) -> Result<tempfile::NamedTempFile, Box<dyn std::error::Error>> {
    let mut temp_file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut temp_file, content.as_bytes())?;
    Ok(temp_file)
}