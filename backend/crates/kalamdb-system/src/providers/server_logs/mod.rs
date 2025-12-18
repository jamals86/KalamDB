//! system.server_logs virtual table
//!
//! Provides read access to server log files in JSON Lines format.
//! Requires `format = "json"` in logging configuration.

use crate::{SystemError, SystemTableProviderExt};
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, Int64Builder, StringBuilder};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use std::any::Any;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// Static schema for system.server_logs
static SERVER_LOGS_SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();

/// Schema helper for system.server_logs
pub struct ServerLogsTableSchema;

impl ServerLogsTableSchema {
    pub fn schema() -> SchemaRef {
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

    pub fn table_name() -> &'static str {
        "server_logs"
    }
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

/// Virtual table that reads server log files
pub struct ServerLogsTableProvider {
    schema: SchemaRef,
    logs_path: PathBuf,
}

impl std::fmt::Debug for ServerLogsTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerLogsTableProvider")
            .field("logs_path", &self.logs_path)
            .finish()
    }
}

impl ServerLogsTableProvider {
    /// Create a new server logs table provider
    pub fn new(logs_path: impl Into<PathBuf>) -> Self {
        Self {
            schema: ServerLogsTableSchema::schema(),
            logs_path: logs_path.into(),
        }
    }

    /// Read and parse log entries from the server.jsonl file
    fn read_log_entries(&self) -> Result<Vec<JsonLogEntry>, SystemError> {
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
            SystemError::Other(format!(
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

    /// Build a RecordBatch from log entries
    fn build_batch(&self) -> Result<RecordBatch, SystemError> {
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

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(timestamps.finish()) as ArrayRef,
                Arc::new(levels.finish()) as ArrayRef,
                Arc::new(threads.finish()) as ArrayRef,
                Arc::new(targets.finish()) as ArrayRef,
                Arc::new(lines.finish()) as ArrayRef,
                Arc::new(messages.finish()) as ArrayRef,
            ],
        )
        .map_err(SystemError::Arrow)?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for ServerLogsTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::View
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;

        let batch = self
            .build_batch()
            .map_err(|e| DataFusionError::External(Box::new(e)))?;

        let mem_table = MemTable::try_new(self.schema.clone(), vec![vec![batch]])?;
        mem_table.scan(_state, projection, _filters, _limit).await
    }
}

#[async_trait]
impl SystemTableProviderExt for ServerLogsTableProvider {
    fn table_name(&self) -> &str {
        ServerLogsTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.build_batch()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_schema() {
        let schema = ServerLogsTableSchema::schema();
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

        let provider = ServerLogsTableProvider::new(dir.path());
        let entries = provider.read_log_entries().unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].level, "INFO");
        assert_eq!(entries[0].message, "Server started");
        assert_eq!(entries[1].level, "DEBUG");
    }

    #[test]
    fn test_empty_log_file() {
        let dir = tempdir().unwrap();
        // Don't create the log file

        let provider = ServerLogsTableProvider::new(dir.path());
        let entries = provider.read_log_entries().unwrap();

        assert!(entries.is_empty());
    }
}
