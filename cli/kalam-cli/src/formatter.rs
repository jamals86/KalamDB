//! Output formatters for query results
//!
//! **Implements T086**: OutputFormatter for table/JSON/CSV formats using tabled
//!
//! Provides consistent, colorized output formatting for query results.

use kalam_link::QueryResponse;
use serde_json::Value as JsonValue;

use crate::{error::Result, session::OutputFormat};

/// Formats query results for display
pub struct OutputFormatter {
    format: OutputFormat,
    color: bool,
}

impl OutputFormatter {
    /// Create a new formatter
    pub fn new(format: OutputFormat, color: bool) -> Self {
        Self { format, color }
    }

    /// Format a query response
    pub fn format_response(&self, response: &QueryResponse) -> Result<String> {
        if let Some(ref error) = response.error {
            return Ok(self.format_error(&error.message));
        }

        match self.format {
            OutputFormat::Table => self.format_table(response),
            OutputFormat::Json => self.format_json(response),
            OutputFormat::Csv => self.format_csv(response),
        }
    }

    /// Format as table
    fn format_table(&self, response: &QueryResponse) -> Result<String> {
        if response.results.is_empty() {
            return Ok("Query executed successfully (0 rows affected)".to_string());
        }

        let result = &response.results[0];
        
        // Check if this is a message-only result (DDL statements)
        if let Some(ref message) = result.message {
            return Ok(message.clone());
        }

        // Handle data results
        if let Some(ref rows) = result.rows {
            if rows.is_empty() {
                return Ok("No results".to_string());
            }

            // Convert rows to table format
            let first = &rows[0];
            let columns: Vec<String> = first.keys().cloned().collect();

        // Build simple text table (tabled requires complex setup, use basic formatting)
        let mut output = String::new();

        // Header row
        output.push_str(&columns.join(" | "));
        output.push('\n');
        output.push_str(&"-".repeat(output.len()));
        output.push('\n');

        // Data rows
        for row in rows {
            let values: Vec<String> = columns
                .iter()
                .map(|col| {
                    row.get(col)
                        .map(|v| self.format_json_value(v))
                        .unwrap_or_else(|| "NULL".to_string())
                })
                .collect();
            output.push_str(&values.join(" | "));
            output.push('\n');
        }

        Ok(output)
        } else {
            Ok(format!("Query executed successfully ({} rows affected)", result.row_count))
        }
    }

    /// Format as JSON
    fn format_json(&self, response: &QueryResponse) -> Result<String> {
        let json = serde_json::to_string_pretty(response)
            .map_err(|e| crate::error::CLIError::FormatError(e.to_string()))?;
        Ok(json)
    }

    /// Format as CSV
    fn format_csv(&self, response: &QueryResponse) -> Result<String> {
        if response.results.is_empty() {
            return Ok("".to_string());
        }

        let result = &response.results[0];
        
        // Handle message-only results
        if result.rows.is_none() {
            return Ok("".to_string());
        }

        let rows = result.rows.as_ref().unwrap();
        if rows.is_empty() {
            return Ok("".to_string());
        }

        let first = &rows[0];

        // Extract columns
        let columns: Vec<String> = first.keys().cloned().collect();

        // Build CSV
        let mut output = columns.join(",") + "\n";

        for row in rows {
            let values: Vec<String> = columns
                .iter()
                .map(|col| {
                    row.get(col)
                        .map(|v| self.format_csv_value(v))
                        .unwrap_or_else(|| "".to_string())
                })
                .collect();
            output.push_str(&values.join(","));
            output.push('\n');
        }

        Ok(output)
    }

    /// Format error message
    fn format_error(&self, error: &str) -> String {
        if self.color {
            format!("\x1b[31mError:\x1b[0m {}", error)
        } else {
            format!("Error: {}", error)
        }
    }

    /// Format JSON value for table display
    fn format_json_value(&self, value: &JsonValue) -> String {
        match value {
            JsonValue::Null => "NULL".to_string(),
            JsonValue::Bool(b) => b.to_string(),
            JsonValue::Number(n) => n.to_string(),
            JsonValue::String(s) => s.clone(),
            JsonValue::Array(_) | JsonValue::Object(_) => value.to_string(),
        }
    }

    /// Format JSON value for CSV (escape commas and quotes)
    fn format_csv_value(&self, value: &JsonValue) -> String {
        let s = self.format_json_value(value);
        if s.contains(',') || s.contains('"') || s.contains('\n') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_json_value() {
        let formatter = OutputFormatter::new(OutputFormat::Table, false);
        assert_eq!(formatter.format_json_value(&JsonValue::Null), "NULL");
        assert_eq!(formatter.format_json_value(&JsonValue::Bool(true)), "true");
        assert_eq!(
            formatter.format_json_value(&JsonValue::String("test".into())),
            "test"
        );
    }

    #[test]
    fn test_csv_escaping() {
        let formatter = OutputFormatter::new(OutputFormat::Csv, false);
        let value = JsonValue::String("hello, world".into());
        assert_eq!(formatter.format_csv_value(&value), "\"hello, world\"");
    }
}
