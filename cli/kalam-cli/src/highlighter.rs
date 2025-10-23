//! SQL syntax highlighting for interactive terminal
//!
//! Provides real-time syntax highlighting for SQL statements and meta-commands
//! with color-coded keywords, strings, numbers, and operators.

use colored::*;
use rustyline::highlight::Highlighter as RustylineHighlighter;
use rustyline::Context;
use std::borrow::Cow;

/// SQL syntax highlighter
pub struct SqlHighlighter {
    enabled: bool,
}

impl SqlHighlighter {
    /// Create a new highlighter
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Check if a word is a SQL keyword
    fn is_keyword(word: &str) -> bool {
        matches!(
            word.to_uppercase().as_str(),
            "SELECT" | "FROM" | "WHERE" | "INSERT" | "INTO" | "VALUES" | "UPDATE" | "SET"
                | "DELETE" | "CREATE" | "DROP" | "ALTER" | "TABLE" | "INDEX" | "VIEW"
                | "DATABASE" | "SCHEMA" | "AND" | "OR" | "NOT" | "IN" | "LIKE" | "BETWEEN"
                | "IS" | "NULL" | "AS" | "ORDER" | "BY" | "GROUP" | "HAVING" | "LIMIT"
                | "OFFSET" | "JOIN" | "LEFT" | "RIGHT" | "INNER" | "OUTER" | "ON"
                | "DISTINCT" | "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" | "CASE" | "WHEN"
                | "THEN" | "ELSE" | "END" | "UNION" | "ALL" | "EXISTS" | "CAST" | "DEFAULT"
                | "PRIMARY" | "KEY" | "FOREIGN" | "REFERENCES" | "UNIQUE" | "CHECK"
                | "CONSTRAINT" | "AUTO_INCREMENT"
        )
    }

    /// Check if a word is a data type
    fn is_type(word: &str) -> bool {
        matches!(
            word.to_uppercase().as_str(),
            "INTEGER" | "INT" | "BIGINT" | "SMALLINT" | "TINYINT" | "TEXT" | "VARCHAR"
                | "CHAR" | "BOOLEAN" | "BOOL" | "TIMESTAMP" | "DATETIME" | "DATE" | "TIME"
                | "FLOAT" | "DOUBLE" | "DECIMAL" | "NUMERIC" | "JSON" | "JSONB" | "BLOB"
                | "BINARY"
        )
    }

    /// Highlight SQL text with colors
    fn highlight_sql(&self, line: &str) -> String {
        if !self.enabled {
            return line.to_string();
        }

        let mut result = String::new();
        let mut chars = line.chars().peekable();
        let mut current_word = String::new();
        let mut in_string = false;
        let mut string_char = ' ';
        let mut in_number = false;

        while let Some(ch) = chars.next() {
            // Handle strings
            if (ch == '\'' || ch == '"') && !in_string {
                // Flush current word
                if !current_word.is_empty() {
                    result.push_str(&self.color_word(&current_word));
                    current_word.clear();
                }
                in_string = true;
                string_char = ch;
                result.push_str(&ch.to_string().green().to_string());
                continue;
            }

            if in_string {
                result.push_str(&ch.to_string().green().to_string());
                if ch == string_char {
                    in_string = false;
                }
                continue;
            }

            // Handle numbers
            if ch.is_ascii_digit() || (ch == '.' && chars.peek().map_or(false, |c| c.is_ascii_digit())) {
                if !in_number {
                    // Flush current word
                    if !current_word.is_empty() {
                        result.push_str(&self.color_word(&current_word));
                        current_word.clear();
                    }
                    in_number = true;
                }
                result.push_str(&ch.to_string().yellow().to_string());
                continue;
            } else if in_number {
                in_number = false;
            }

            // Handle operators and punctuation
            if matches!(ch, '(' | ')' | ',' | ';' | '=' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '!' | '&' | '|') {
                // Flush current word
                if !current_word.is_empty() {
                    result.push_str(&self.color_word(&current_word));
                    current_word.clear();
                }
                result.push_str(&ch.to_string().cyan().bold().to_string());
                continue;
            }

            // Handle whitespace
            if ch.is_whitespace() {
                if !current_word.is_empty() {
                    result.push_str(&self.color_word(&current_word));
                    current_word.clear();
                }
                result.push(ch);
                continue;
            }

            // Build up word
            current_word.push(ch);
        }

        // Flush remaining word
        if !current_word.is_empty() {
            result.push_str(&self.color_word(&current_word));
        }

        result
    }

    /// Color a word based on its type
    fn color_word(&self, word: &str) -> String {
        if Self::is_keyword(word) {
            word.blue().bold().to_string()
        } else if Self::is_type(word) {
            word.magenta().bold().to_string()
        } else {
            word.normal().to_string()
        }
    }
}

impl RustylineHighlighter for SqlHighlighter {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        // Disable highlighting for better performance
        // The constant repainting causes cursor issues and lag
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        // Disable character-by-character highlighting
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_detection() {
        assert!(SqlHighlighter::is_keyword("SELECT"));
        assert!(SqlHighlighter::is_keyword("select"));
        assert!(!SqlHighlighter::is_keyword("user_name"));
    }

    #[test]
    fn test_type_detection() {
        assert!(SqlHighlighter::is_type("INTEGER"));
        assert!(SqlHighlighter::is_type("varchar"));
        assert!(!SqlHighlighter::is_type("SELECT"));
    }
}
