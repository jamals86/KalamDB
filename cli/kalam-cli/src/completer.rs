//! TAB completion for SQL keywords and table names
//!
//! **Implements T088**: AutoCompleter for rustyline TAB completion
//! **Implements T114b**: Enhanced autocomplete with table and column names
//! **Enhanced**: Styled completions with Warp-like design
//!
//! Provides intelligent autocompletion for SQL keywords, table names, column names,
//! and backslash commands with context-aware suggestions and beautiful styling.

use rustyline::completion::{Completer, Pair};
use std::collections::HashMap;
use colored::*;

/// Styled completion candidate
#[derive(Debug, Clone)]
pub struct StyledPair {
    /// Display text (with styling)
    display: String,
    /// Replacement text (plain)
    replacement: String,
}

impl StyledPair {
    fn new(text: String, category: CompletionCategory) -> Self {
        let display = match category {
            CompletionCategory::Keyword => format!("{}  {}", text.blue().bold(), "keyword".dimmed()),
            CompletionCategory::Table => format!("{}  {}", text.green(), "table".dimmed()),
            CompletionCategory::Column => format!("{}  {}", text.yellow(), "column".dimmed()),
            CompletionCategory::MetaCommand => format!("{}  {}", text.cyan().bold(), "command".dimmed()),
            CompletionCategory::Type => format!("{}  {}", text.magenta(), "type".dimmed()),
        };

        Self {
            display,
            replacement: text,
        }
    }
}

/// Category of completion for styling
#[derive(Debug, Clone, Copy)]
enum CompletionCategory {
    Keyword,
    Table,
    Column,
    MetaCommand,
    Type,
}

/// Auto-completer for SQL and meta-commands
pub struct AutoCompleter {
    /// SQL keywords for completion
    keywords: Vec<String>,

    /// Meta-commands for completion
    meta_commands: Vec<String>,

    /// Cached table names
    tables: Vec<String>,

    /// Cached column names per table (table_name -> Vec<column_name>)
    columns: HashMap<String, Vec<String>>,
}

impl AutoCompleter {
    /// Create a new auto-completer
    pub fn new() -> Self {
        let mut keywords = vec![
            // DML
            "SELECT", "INSERT", "UPDATE", "DELETE", "FROM", "WHERE", "JOIN", "ON", "AND", "OR",
            "NOT", "IN", "LIKE", "BETWEEN", "IS", "NULL", "AS", "ORDER", "BY", "GROUP", "HAVING",
            "LIMIT", "OFFSET", "DISTINCT", "VALUES", "SET",
            // DDL
            "CREATE", "DROP", "ALTER", "TABLE", "INDEX", "VIEW", "DATABASE", "SCHEMA",
            // Constraints
            "PRIMARY", "KEY", "FOREIGN", "REFERENCES", "UNIQUE", "NOT", "NULL", "DEFAULT",
            "CHECK", "AUTO_INCREMENT",
            // Functions
            "COUNT", "SUM", "AVG", "MIN", "MAX", "COALESCE", "CAST", "CONCAT", "LENGTH",
            "UPPER", "LOWER", "TRIM", "NOW", "CURRENT_TIMESTAMP",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

        let types = vec![
            "INTEGER", "BIGINT", "TEXT", "VARCHAR", "BOOLEAN", "TIMESTAMP", "FLOAT", "DOUBLE",
            "JSON",
        ];
        keywords.extend(types.iter().map(|s| s.to_string()));

        let meta_commands = vec![
            "\\quit",
            "\\q",
            "\\help",
            "\\?",
            "\\connect",
            "\\c",
            "\\config",
            "\\flush",
            "\\health",
            "\\pause",
            "\\continue",
            "\\dt",
            "\\tables",
            "\\d",
            "\\describe",
            "\\format",
            "\\refresh-tables",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            keywords,
            meta_commands,
            tables: Vec::new(),
            columns: HashMap::new(),
        }
    }

    /// Update cached table names
    pub fn set_tables(&mut self, tables: Vec<String>) {
        self.tables = tables;
    }

    /// Update cached column names for a table
    pub fn set_columns(&mut self, table: String, cols: Vec<String>) {
        self.columns.insert(table, cols);
    }

    /// Clear all cached column information
    pub fn clear_columns(&mut self) {
        self.columns.clear();
    }

    /// Detect completion context from the line
    fn detect_context(&self, line: &str) -> CompletionContext {
        let line_upper = line.to_uppercase();
        
        // Check if we're after FROM or JOIN (table name context)
        if line_upper.contains(" FROM ") || line_upper.contains(" JOIN ") {
            // Check if there's a dot (table.column context)
            if let Some(dot_pos) = line.rfind('.') {
                // Extract table name before the dot
                let before_dot = &line[..dot_pos];
                if let Some(word_start) = before_dot.rfind(|c: char| c.is_whitespace() || c == '(' || c == ',') {
                    let table_name = before_dot[word_start + 1..].trim().to_string();
                    return CompletionContext::Column(table_name);
                }
            }
            return CompletionContext::Table;
        }
        
        // Default to keyword/table mixed context
        CompletionContext::Mixed
    }

    /// Get completions with styling for display
    fn get_styled_completions(&self, input: &str, line: &str) -> Vec<StyledPair> {
        let input_upper = input.to_uppercase();
        let mut results = Vec::new();

        // Check for meta-commands
        if input.starts_with('\\') {
            for cmd in &self.meta_commands {
                if cmd.to_uppercase().starts_with(&input_upper) {
                    results.push(StyledPair::new(cmd.clone(), CompletionCategory::MetaCommand));
                }
            }
            return results;
        }

        let context = self.detect_context(line);
        
        match context {
            CompletionContext::Table => {
                // Only suggest table names
                for table in &self.tables {
                    if table.to_uppercase().starts_with(&input_upper) {
                        results.push(StyledPair::new(table.clone(), CompletionCategory::Table));
                    }
                }
            }
            CompletionContext::Column(ref table_name) => {
                // Only suggest column names for the specific table
                if let Some(cols) = self.columns.get(table_name) {
                    for col in cols {
                        if col.to_uppercase().starts_with(&input_upper) {
                            results.push(StyledPair::new(col.clone(), CompletionCategory::Column));
                        }
                    }
                }
            }
            CompletionContext::Mixed => {
                // Suggest keywords
                for kw in &self.keywords {
                    if kw.starts_with(&input_upper) {
                        let category = if Self::is_type(kw) {
                            CompletionCategory::Type
                        } else {
                            CompletionCategory::Keyword
                        };
                        results.push(StyledPair::new(kw.clone(), category));
                    }
                }

                // Suggest table names
                for table in &self.tables {
                    if table.to_uppercase().starts_with(&input_upper) {
                        results.push(StyledPair::new(table.clone(), CompletionCategory::Table));
                    }
                }
            }
        }

        results.sort_by(|a, b| a.replacement.cmp(&b.replacement));
        results.dedup_by(|a, b| a.replacement == b.replacement);
        results
    }

    /// Check if a keyword is a type
    fn is_type(word: &str) -> bool {
        matches!(
            word,
            "INTEGER" | "BIGINT" | "TEXT" | "VARCHAR" | "BOOLEAN" | "TIMESTAMP" | "FLOAT" | "DOUBLE" | "JSON"
        )
    }
}

/// Completion context for context-aware suggestions
#[derive(Debug)]
enum CompletionContext {
    /// Mixed keywords and tables
    Mixed,
    /// Table name context (after FROM/JOIN)
    Table,
    /// Column name context (after table.)
    Column(String),
}

impl Default for AutoCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl Completer for AutoCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Find the start of the current word
        let start = line[..pos]
            .rfind(|c: char| c.is_whitespace() || c == '(' || c == '.')
            .map(|i| i + 1)
            .unwrap_or(0);

        let word = &line[start..pos];
        let styled_completions = self.get_styled_completions(word, line);

        let pairs: Vec<Pair> = styled_completions
            .into_iter()
            .map(|s| Pair {
                display: s.display,
                replacement: s.replacement,
            })
            .collect();

        Ok((start, pairs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_completion() {
        let completer = AutoCompleter::new();
        let completions = completer.get_styled_completions("SEL", "SEL");
        assert!(completions.iter().any(|c| c.replacement == "SELECT"));
    }

    #[test]
    fn test_meta_command_completion() {
        let completer = AutoCompleter::new();
        let completions = completer.get_styled_completions("\\q", "\\q");
        assert!(completions.iter().any(|c| c.replacement == "\\quit"));
        assert!(completions.iter().any(|c| c.replacement == "\\q"));
    }

    #[test]
    fn test_table_completion() {
        let mut completer = AutoCompleter::new();
        completer.set_tables(vec!["users".to_string(), "user_sessions".to_string()]);

        let completions = completer.get_styled_completions("user", "user");
        assert!(completions.iter().any(|c| c.replacement == "users"));
        assert!(completions.iter().any(|c| c.replacement == "user_sessions"));
    }

    #[test]
    fn test_context_aware_table_completion() {
        let mut completer = AutoCompleter::new();
        completer.set_tables(vec!["users".to_string(), "messages".to_string()]);

        // After FROM, should only suggest tables
        let completions = completer.get_styled_completions("me", "SELECT * FROM me");
        assert!(completions.iter().any(|c| c.replacement == "messages"));
    }

    #[test]
    fn test_column_completion() {
        let mut completer = AutoCompleter::new();
        completer.set_tables(vec!["users".to_string()]);
        completer.set_columns("users".to_string(), vec!["id".to_string(), "name".to_string(), "email".to_string()]);

        // After table., should suggest columns (with FROM keyword present)
        let completions = completer.get_styled_completions("na", "SELECT users.na FROM users");
        assert!(completions.iter().any(|c| c.replacement == "name"));
    }
}
