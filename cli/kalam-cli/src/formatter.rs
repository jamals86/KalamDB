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
                // For empty results, use columns from result metadata
                result.columns.clone()
            } else {
                // Get columns from first row
                rows[0].keys().cloned().collect()
            };

            // Calculate column widths for alignment
            let mut col_widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
            
            // Update widths based on data
            for row in rows {
                for (i, col) in columns.iter().enumerate() {
                    let value = row.get(col)
                        .map(|v| self.format_json_value(v))
                        .unwrap_or_else(|| "NULL".to_string());
                    col_widths[i] = col_widths[i].max(value.len());
                }
            }

            // Build aligned text table
            let mut output = String::new();

            // Header row with proper spacing
            let header_parts: Vec<String> = columns.iter()
                .zip(col_widths.iter())
                .map(|(col, width)| format!("{:width$}", col, width = width))
                .collect();
            output.push_str(&header_parts.join(" | "));
            output.push('\n');
            
            // Separator line
            let total_width: usize = col_widths.iter().sum::<usize>() + (columns.len() - 1) * 3;
            output.push_str(&"-".repeat(total_width));
            output.push('\n');

            // Data rows with proper spacing
            for row in rows {
                let value_parts: Vec<String> = columns.iter()
                    .zip(col_widths.iter())
                    .map(|(col, width)| {
                        let value = row.get(col)
                            .map(|v| self.format_json_value(v))
                            .unwrap_or_else(|| "NULL".to_string());
                        format!("{:width$}", value, width = width)
                    })
                    .collect();
                output.push_str(&value_parts.join(" | "));
                output.push('\n');
            }

            // Add row count summary (MySQL/PostgreSQL style)
            let row_count = rows.len();
            if row_count == 0 {
                output.push_str(&format!("Empty set ({:.2} sec)", exec_time));
            } else if row_count == 1 {
                output.push_str(&format!("1 row in set ({:.2} sec)", exec_time));
            } else {
                output.push_str(&format!("{} rows in set ({:.2} sec)", row_count, exec_time));
            }

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
