//! system.server_logs virtual view
//!
//! **Type**: Virtual View (not backed by persistent storage)
//!
//! Provides read access to server log files in JSON Lines format.
//! Requires `format = "json"` in logging configuration.
//!
//! **DataFusion Pattern**: Implements VirtualView trait for consistent view behavior
//! - Reads log files dynamically on each query (no cached state)
//! - Only used in development environments
//! - No memory consumption when idle

use super::view_base::{VirtualView, ViewTableProvider};
use crate::schema_registry::RegistryError;
use datafusion::arrow::array::{ArrayRef, Int64Builder, StringBuilder};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// Static schema for system.server_logs
static SERVER_LOGS_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Get or initialize the server_logs schema
fn server_logs_schema() -> SchemaRef {
    SERVER_LOGS_SCHEMA
        .get_or_init(|| {
            Arc::new(Schema::new(vec![
                Field::new("timestamp", DataType::Utf8, false),
                Field::new("level", DataType::Utf8, false),
                Field::new("thread", DataType::Utf8, true),
                Field::new("target", DataType::Utf8, true),
                Field::new("line", DataType::Int64, true),
                Field::new("message", DataType::Utf8, false),
            ]))
        })
        .clone()
}

/// JSON log entry structure (matches the logging format)
#[derive(Debug, serde::Deserialize)]
struct JsonLogEntry {
    timestamp: String,
    level: String,
    thread: Option<String>,
    target: Option<String>,
    line: Option<i64>,
    message: String,
}

/// ServerLogsView - Reads server log files dynamically
#[derive(Debug)]
pub struct ServerLogsView {
    logs_path: PathBuf,
}

impl ServerLogsView {
    /// Create a new server logs view
    pub fn new(logs_path: impl Into<PathBuf>) -> Self {
        Self {
            logs_path: logs_path.into(),
        }
    }

    /// Read and parse log entries from the server.jsonl file
    fn read_log_entries(&self) -> Result<Vec<JsonLogEntry>, RegistryError> {
        // Try .jsonl first (JSON format), fallback to .log (compact format)
        let jsonl_path = self.logs_path.join("server.jsonl");
        let log_file_path = if jsonl_path.exists() {
            jsonl_path
        } else {
            self.logs_path.join("server.log")
        };

        if !log_file_path.exists() {
            // No log file yet, return empty
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&log_file_path).map_err(|e| {
            RegistryError::Other(format!(
                "Failed to read log file {}: {}",
                log_file_path.display(),
                e
            ))
        })?;

        let mut entries = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Try to parse as JSON
            match serde_json::from_str::<JsonLogEntry>(line) {
                Ok(entry) => entries.push(entry),
                Err(_) => {
                    // Skip non-JSON lines (e.g., if format is "compact")
                    // This allows graceful degradation
                    continue;
                }
            }
        }

        Ok(entries)
    }
}

impl VirtualView for ServerLogsView {
    fn schema(&self) -> SchemaRef {
        server_logs_schema()
    }

    fn compute_batch(&self) -> Result<RecordBatch, RegistryError> {
        let entries = self.read_log_entries()?;

        let mut timestamps = StringBuilder::new();
        let mut levels = StringBuilder::new();
        let mut threads = StringBuilder::new();
        let mut targets = StringBuilder::new();
        let mut lines = Int64Builder::new();
        let mut messages = StringBuilder::new();

        for entry in entries {
            timestamps.append_value(&entry.timestamp);
            levels.append_value(&entry.level);

            if let Some(thread) = &entry.thread {
                threads.append_value(thread);
            } else {
                threads.append_null();
            }

            if let Some(target) = &entry.target {
                targets.append_value(target);
            } else {
                targets.append_null();
            }

            if let Some(line) = entry.line {
                lines.append_value(line);
            } else {
                lines.append_null();
            }

            messages.append_value(&entry.message);
        }

        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(timestamps.finish()) as ArrayRef,
                Arc::new(levels.finish()) as ArrayRef,
                Arc::new(threads.finish()) as ArrayRef,
                Arc::new(targets.finish()) as ArrayRef,
                Arc::new(lines.finish()) as ArrayRef,
                Arc::new(messages.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| RegistryError::Other(format!("Failed to build server_logs batch: {}", e)))
    }

    fn view_name(&self) -> &str {
        "system.server_logs"
    }
}

/// Type alias for the server logs table provider
pub type ServerLogsTableProvider = ViewTableProvider<ServerLogsView>;

/// Helper function to create a server logs table provider
pub fn create_server_logs_provider(logs_path: impl Into<PathBuf>) -> ServerLogsTableProvider {
    ViewTableProvider::new(Arc::new(ServerLogsView::new(logs_path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_schema() {
        let schema = server_logs_schema();
        assert_eq!(schema.fields().len(), 6);
        assert_eq!(schema.field(0).name(), "timestamp");
        assert_eq!(schema.field(1).name(), "level");
        assert_eq!(schema.field(2).name(), "thread");
        assert_eq!(schema.field(3).name(), "target");
        assert_eq!(schema.field(4).name(), "line");
        assert_eq!(schema.field(5).name(), "message");
    }

    #[test]
    fn test_parse_json_logs() {
        let dir = tempdir().unwrap();
        let log_file = dir.path().join("server.log");

        // Write some JSON log entries
        let mut file = std::fs::File::create(&log_file).unwrap();
        writeln!(
            file,
            r#"{{"timestamp":"2024-01-15T10:30:00.123+00:00","level":"INFO","thread":"main","target":"kalamdb_server","line":42,"message":"Server started"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"timestamp":"2024-01-15T10:30:01.456+00:00","level":"DEBUG","thread":"worker-1","target":"kalamdb_core","line":100,"message":"Processing query"}}"#
        )
        .unwrap();

        let view = ServerLogsView::new(dir.path());
        let entries = view.read_log_entries().unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].level, "INFO");
        assert_eq!(entries[0].message, "Server started");
        assert_eq!(entries[1].level, "DEBUG");
    }

    #[test]
    fn test_empty_log_file() {
        let dir = tempdir().unwrap();
        // Don't create the log file

        let view = ServerLogsView::new(dir.path());
        let entries = view.read_log_entries().unwrap();

        assert!(entries.is_empty());
    }

    #[test]
    fn test_compute_batch() {
        let dir = tempdir().unwrap();
        let log_file = dir.path().join("server.jsonl");

        let mut file = std::fs::File::create(&log_file).unwrap();
        writeln!(
            file,
            r#"{{"timestamp":"2024-01-15T10:30:00Z","level":"INFO","message":"Test"}}"#
        )
        .unwrap();

        let view = ServerLogsView::new(dir.path());
        let batch = view.compute_batch().unwrap();

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 6);
    }
}
