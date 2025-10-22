//! Output formatters for query results
//!
//! **Implements T086**: OutputFormatter for table/JSON/CSV formats using tabled
//!
//! Provides consistent, colorized output formatting for query results.

use kalam_link::{QueryResponse, ErrorDetail};
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
            return Ok(self.format_error_detail(error));
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
            let exec_time = response.execution_time_ms.unwrap_or(0) as f64 / 1000.0;
            return Ok(format!("Query OK, 0 rows affected ({:.2} sec)", exec_time));
        }

        let result = &response.results[0];
        let exec_time = response.execution_time_ms.unwrap_or(0) as f64 / 1000.0;
        
        // Check if this is a message-only result (DDL statements)
        if let Some(ref message) = result.message {
            // Format DDL message similar to MySQL/PostgreSQL
            let row_count = result.row_count;
            return Ok(format!("{}\nQuery OK, {} rows affected ({:.2} sec)", message, row_count, exec_time));
        }

        // Handle data results
        if let Some(ref rows) = result.rows {
            let columns: Vec<String> = if rows.is_empty() {
                result.columns.clone()
            } else {
                rows[0].keys().cloned().collect()
            };

            let mut col_widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
            for row in rows {
                for (i, col) in columns.iter().enumerate() {
                    let value = row
                        .get(col)
                        .map(|v| self.format_json_value(v))
                        .unwrap_or_else(|| "NULL".to_string());
                    col_widths[i] = col_widths[i].max(value.len());
                }
            }

            let mut output = String::new();

            // Top border
            output.push('┌');
            for (idx, width) in col_widths.iter().enumerate() {
                output.push_str(&"─".repeat(width + 2));
                output.push(if idx == col_widths.len() - 1 { '┐' } else { '┬' });
            }
            output.push('\n');

            // Header row
            output.push('│');
            for (i, col) in columns.iter().enumerate() {
                output.push(' ');
                output.push_str(&format!("{:width$}", col, width = col_widths[i]));
                output.push(' ');
                output.push('│');
            }
            output.push('\n');

            // Header separator
            output.push('├');
            for (idx, width) in col_widths.iter().enumerate() {
                output.push_str(&"─".repeat(width + 2));
                output.push(if idx == col_widths.len() - 1 { '┤' } else { '┼' });
            }
            output.push('\n');

            // Data rows
            for row in rows {
                output.push('│');
                for (i, col) in columns.iter().enumerate() {
                    let value = row
                        .get(col)
                        .map(|v| self.format_json_value(v))
                        .unwrap_or_else(|| "NULL".to_string());
                    output.push(' ');
                    output.push_str(&format!("{:width$}", value, width = col_widths[i]));
                    output.push(' ');
                    output.push('│');
                }
                output.push('\n');
            }

            // Bottom border
            output.push('└');
            for (idx, width) in col_widths.iter().enumerate() {
                output.push_str(&"─".repeat(width + 2));
                output.push(if idx == col_widths.len() - 1 { '┘' } else { '┴' });
            }
            output.push('\n');

            let row_count = rows.len();
            let row_label = if row_count == 1 { "row" } else { "rows" };
            output.push_str(&format!("({} {})\n", row_count, row_label));
            output.push_str(&format!("Time: {:.3} s", exec_time));

            Ok(output)
        } else {
            // Non-query statement (INSERT, UPDATE, DELETE)
            let row_count = result.row_count;
            Ok(format!("Query OK, {} rows affected ({:.2} sec)", row_count, exec_time))
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

    /// Format error message (simple version)
    fn format_error(&self, error: &str) -> String {
        if self.color {
            format!("\x1b[31mERROR:\x1b[0m {}", error)
        } else {
            format!("ERROR: {}", error)
        }
    }

    /// Format error detail (with code and details) - MySQL/PostgreSQL style
    fn format_error_detail(&self, error: &ErrorDetail) -> String {
        let mut output = String::new();
        
        if self.color {
            output.push_str(&format!("\x1b[31mERROR {}\x1b[0m: {}\n", error.code, error.message));
        } else {
            output.push_str(&format!("ERROR {}: {}\n", error.code, error.message));
        }
        
        if let Some(ref details) = error.details {
            output.push_str(&format!("Details: {}", details));
        }
        
        output
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
