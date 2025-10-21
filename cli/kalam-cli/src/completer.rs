//! TAB completion for SQL keywords and table names
//!
//! **Implements T088**: AutoCompleter for rustyline TAB completion
//!
//! Provides intelligent autocompletion for SQL keywords, table names, and backslash commands.

use rustyline::completion::{Completer, Pair};
use rustyline::Context;

/// Auto-completer for SQL and meta-commands
pub struct AutoCompleter {
    /// SQL keywords for completion
    keywords: Vec<String>,

    /// Meta-commands for completion
    meta_commands: Vec<String>,

    /// Cached table names
    tables: Vec<String>,
}

impl AutoCompleter {
    /// Create a new auto-completer
    pub fn new() -> Self {
        let keywords = vec![
            // DML
            "SELECT", "INSERT", "UPDATE", "DELETE", "FROM", "WHERE", "JOIN", "ON", "AND", "OR",
            "NOT", "IN", "LIKE", "BETWEEN", "IS", "NULL", "AS", "ORDER", "BY", "GROUP", "HAVING",
            "LIMIT", "OFFSET", "DISTINCT", "VALUES", "SET",
            // DDL
            "CREATE", "DROP", "ALTER", "TABLE", "INDEX", "VIEW", "DATABASE", "SCHEMA",
            // Types
            "INTEGER", "BIGINT", "TEXT", "VARCHAR", "BOOLEAN", "TIMESTAMP", "FLOAT", "DOUBLE",
            "JSON",
            // Constraints
            "PRIMARY", "KEY", "FOREIGN", "REFERENCES", "UNIQUE", "NOT", "NULL", "DEFAULT",
            "CHECK", "AUTO_INCREMENT",
            // Functions
            "COUNT", "SUM", "AVG", "MIN", "MAX", "COALESCE", "CAST", "CONCAT", "LENGTH",
            "UPPER", "LOWER", "TRIM", "NOW", "CURRENT_TIMESTAMP",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

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
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            keywords,
            meta_commands,
            tables: Vec::new(),
        }
    }

    /// Update cached table names
    pub fn set_tables(&mut self, tables: Vec<String>) {
        self.tables = tables;
    }

    /// Get completions for a given input
    fn get_completions(&self, input: &str) -> Vec<String> {
        let input_upper = input.to_uppercase();

        // Check for meta-commands
        if input.starts_with('\\') {
            return self
                .meta_commands
                .iter()
                .filter(|cmd| cmd.to_uppercase().starts_with(&input_upper))
                .cloned()
                .collect();
        }

        // Check for SQL keywords
        let mut completions: Vec<String> = self
            .keywords
            .iter()
            .filter(|kw| kw.starts_with(&input_upper))
            .cloned()
            .collect();

        // Check for table names
        completions.extend(
            self.tables
                .iter()
                .filter(|tbl| tbl.to_uppercase().starts_with(&input_upper))
                .cloned(),
        );

        completions.sort();
        completions.dedup();
        completions
    }
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
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Find the start of the current word
        let start = line[..pos]
            .rfind(|c: char| c.is_whitespace() || c == '(')
            .map(|i| i + 1)
            .unwrap_or(0);

        let word = &line[start..pos];
        let completions = self.get_completions(word);

        let pairs: Vec<Pair> = completions
            .into_iter()
            .map(|s| Pair {
                display: s.clone(),
                replacement: s,
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
        let completions = completer.get_completions("SEL");
        assert!(completions.contains(&"SELECT".to_string()));
    }

    #[test]
    fn test_meta_command_completion() {
        let completer = AutoCompleter::new();
        let completions = completer.get_completions("\\q");
        assert!(completions.contains(&"\\quit".to_string()));
        assert!(completions.contains(&"\\q".to_string()));
    }

    #[test]
    fn test_table_completion() {
        let mut completer = AutoCompleter::new();
        completer.set_tables(vec!["users".to_string(), "user_sessions".to_string()]);

        let completions = completer.get_completions("user");
        assert!(completions.contains(&"users".to_string()));
        assert!(completions.contains(&"user_sessions".to_string()));
    }
}
