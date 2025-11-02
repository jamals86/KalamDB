#![allow(dead_code, unused_imports)]
use assert_cmd;
use std::io::{BufRead, BufReader};
use std::process::Command;
use std::process::{Child, Stdio};
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

// Re-export commonly used types for credential tests
pub use kalam_cli::FileCredentialStore;
pub use kalam_link::credentials::{CredentialStore, Credentials};
pub use tempfile::TempDir;

#[cfg(unix)]
pub use std::os::unix::fs::PermissionsExt;

pub const SERVER_URL: &str = "http://localhost:8080";
pub const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Check if the KalamDB server is running
/// Check if the KalamDB server is running
pub fn is_server_running() -> bool {
    // Simple TCP connection check
    std::net::TcpStream::connect("localhost:8080")
        .map(|_| true)
        .unwrap_or(false)
}

/// Helper to execute SQL via CLI
pub fn execute_sql_via_cli(sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_kalam"))
        .arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(sql)
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "CLI command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

/// Helper to execute SQL via CLI with authentication
pub fn execute_sql_via_cli_as(
    username: &str,
    password: &str,
    sql: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_kalam"))
        .arg("-u")
        .arg(SERVER_URL)
        .arg("--username")
        .arg(username)
        .arg("--password")
        .arg(password)
        .arg("--command")
        .arg(sql)
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "CLI command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
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

/// Helper to create a CLI command with default test settings
/// Helper to create a CLI command with default test settings (for assert_cmd tests)
pub fn create_cli_command() -> assert_cmd::Command {
    assert_cmd::Command::new(env!("CARGO_BIN_EXE_kalam"))
}

/// Helper to create a temporary credential store for testing
pub fn create_temp_store() -> (FileCredentialStore, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let creds_path = temp_dir.path().join("credentials.toml");

    let store =
        FileCredentialStore::with_path(creds_path).expect("Failed to create credential store");

    (store, temp_dir)
}

/// Helper to setup test namespace and table with unique name
pub fn setup_test_table(test_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let table_name = generate_unique_table(test_name);
    let namespace = "test_cli";
    let full_table_name = format!("{}.{}", namespace, table_name);

    // Small delay to avoid race conditions
    std::thread::sleep(Duration::from_millis(100));

    // Try to drop table first if it exists
    let drop_sql = format!("DROP TABLE IF EXISTS {}", full_table_name);
    let _ = execute_sql_as_root_via_cli(&drop_sql);
    std::thread::sleep(Duration::from_millis(50));

    // Create namespace if it doesn't exist
    let ns_sql = format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace);
    let _ = execute_sql_as_root_via_cli(&ns_sql);
    std::thread::sleep(Duration::from_millis(100));

    // Create test table
    let create_sql = format!(
        r#"CREATE USER TABLE {} (
            id INT AUTO_INCREMENT,
            content VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
        full_table_name
    );

    execute_sql_as_root_via_cli(&create_sql)?;
    std::thread::sleep(Duration::from_millis(100));

    Ok(full_table_name)
}

/// Helper to cleanup test data
pub fn cleanup_test_table(table_full_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let drop_sql = format!("DROP TABLE IF EXISTS {}", table_full_name);
    let _ = execute_sql_as_root_via_cli(&drop_sql);
    std::thread::sleep(Duration::from_millis(50));
    Ok(())
}

/// Subscription listener for testing real-time events via CLI
pub struct SubscriptionListener {
    child: Child,
    stdout_reader: Option<BufReader<std::process::ChildStdout>>,
    line_receiver: Option<std_mpsc::Receiver<Result<String, String>>>,
    reader_thread: Option<thread::JoinHandle<()>>,
}

impl SubscriptionListener {
    /// Start a subscription listener for a given query
    pub fn start(query: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kalam"))
            .arg("-u")
            .arg(SERVER_URL)
            .arg("--subscribe")
            .arg(query)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let mut stdout_reader = BufReader::new(stdout);

        // Start a background thread to read lines with ability to timeout
        let (tx, rx) = std_mpsc::channel();
        let reader_thread = thread::spawn(move || {
            loop {
                let mut line = String::new();
                match stdout_reader.read_line(&mut line) {
                    Ok(0) => {
                        // EOF
                        let _ = tx.send(Ok("".to_string()));
                        break;
                    }
                    Ok(_) => {
                        if tx.send(Ok(line.trim().to_string())).is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child,
            stdout_reader: None,
            line_receiver: Some(rx),
            reader_thread: Some(reader_thread),
        })
    }

    /// Read next line from subscription output
    pub fn read_line(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(ref rx) = self.line_receiver {
            match rx.recv() {
                Ok(Ok(line)) => {
                    if line.is_empty() {
                        Ok(None) // EOF
                    } else {
                        Ok(Some(line))
                    }
                }
                Ok(Err(e)) => Err(e.into()),
                Err(_) => Ok(None), // Channel closed
            }
        } else {
            Err("Listener not initialized properly".into())
        }
    }

    /// Try to read a line with a timeout (uses polling approach)
    pub fn try_read_line(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(ref rx) = self.line_receiver {
            match rx.recv_timeout(timeout) {
                Ok(Ok(line)) => {
                    if line.is_empty() {
                        Ok(None) // EOF
                    } else {
                        Ok(Some(line))
                    }
                }
                Ok(Err(e)) => Err(e.into()),
                Err(std_mpsc::RecvTimeoutError::Timeout) => {
                    Err("Timeout waiting for subscription data".into())
                }
                Err(std_mpsc::RecvTimeoutError::Disconnected) => Ok(None),
            }
        } else {
            Err("Listener not initialized properly".into())
        }
    }

    /// Wait for a specific event pattern
    pub fn wait_for_event(
        &mut self,
        pattern: &str,
        timeout: Duration,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            match self.try_read_line(Duration::from_millis(100)) {
                Ok(Some(line)) => {
                    if line.contains(pattern) {
                        return Ok(line);
                    }
                }
                Ok(None) => break,  // EOF
                Err(_) => continue, // Timeout on this read, try again
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
pub fn start_subscription_listener(
    query: &str,
) -> Result<std_mpsc::Receiver<String>, Box<dyn std::error::Error>> {
    let (event_sender, event_receiver) = std_mpsc::channel();
    let query = query.to_string();

    thread::spawn(move || {
        let mut listener = match SubscriptionListener::start(&query) {
            Ok(l) => l,
            Err(e) => {
                let _ = event_sender.send(format!("ERROR: {}", e));
                return;
            }
        };

        loop {
            match listener.try_read_line(Duration::from_millis(100)) {
                Ok(Some(line)) => {
                    let _ = event_sender.send(line);
                }
                Ok(None) => break,  // EOF
                Err(_) => continue, // Timeout, try again
            }
        }
    });

    Ok(event_receiver)
}
