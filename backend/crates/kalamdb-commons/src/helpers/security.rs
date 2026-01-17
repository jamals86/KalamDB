//! Security utilities for KalamDB
//!
//! This module provides security-related utility functions used across the codebase.

/// Redacts sensitive information from SQL statements before logging.
///
/// # Security
/// This function removes passwords and other sensitive data from SQL statements
/// to prevent accidental exposure in log files.
///
/// Currently redacts:
/// - `SET PASSWORD 'value'` → `SET PASSWORD '[REDACTED]'`
/// - `PASSWORD 'value'` → `PASSWORD '[REDACTED]'`
/// - `IDENTIFIED BY 'value'` → `IDENTIFIED BY '[REDACTED]'`
///
/// # Example
/// ```
/// use kalamdb_commons::security::redact_sensitive_sql;
///
/// let sql = "ALTER USER 'alice' SET PASSWORD 'secret123'";
/// let safe = redact_sensitive_sql(sql);
/// assert!(!safe.contains("secret123"));
/// assert!(safe.contains("[REDACTED]"));
/// ```
pub fn redact_sensitive_sql(sql: &str) -> String {
    // Fast path: check if SQL contains any password-related keywords
    let sql_upper = sql.to_ascii_uppercase();
    if !sql_upper.contains("PASSWORD") && !sql_upper.contains("IDENTIFIED") {
        return sql.to_string();
    }

    // Use simple string matching for common patterns
    // This is more reliable than regex and has no external dependencies
    let mut result = sql.to_string();

    // Pattern matching for common password syntaxes
    // We look for PASSWORD followed by a quoted string
    result = redact_pattern(&result, "SET PASSWORD ", &['\'', '"']);
    result = redact_pattern(&result, "PASSWORD ", &['\'', '"']);
    result = redact_pattern(&result, "IDENTIFIED BY ", &['\'', '"']);

    result
}

/// Redacts a quoted value after a specific pattern.
fn redact_pattern(sql: &str, pattern: &str, quotes: &[char]) -> String {
    let sql_upper = sql.to_uppercase();

    // Find pattern case-insensitively
    if let Some(pattern_pos) = sql_upper.find(&pattern.to_ascii_uppercase()) {
        let after_pattern = pattern_pos + pattern.len();
        if after_pattern < sql.len() {
            let remainder = &sql[after_pattern..];
            let trimmed = remainder.trim_start();
            let whitespace_len = remainder.len() - trimmed.len();

            // Check if next character is a quote
            if let Some(first_char) = trimmed.chars().next() {
                if quotes.contains(&first_char) {
                    // Find the closing quote
                    let quote_start = after_pattern + whitespace_len;
                    if let Some(quote_end) = find_closing_quote(&sql[quote_start + 1..], first_char)
                    {
                        let actual_end = quote_start + 1 + quote_end + 1;
                        // Replace the quoted value
                        let before = &sql[..quote_start];
                        let after = &sql[actual_end..];
                        return format!("{}'{}'{}[REDACTED]'{}'", before, "", "", after);
                    }
                }
            }
        }
    }

    sql.to_string()
}

/// Finds the position of the closing quote, handling escaped quotes.
fn find_closing_quote(s: &str, quote: char) -> Option<usize> {
    let mut chars = s.chars().enumerate();
    while let Some((i, c)) = chars.next() {
        if c == quote {
            // Check if it's an escaped quote (doubled)
            if s[i + 1..].starts_with(quote) {
                chars.next(); // Skip the escaped quote
            } else {
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_set_password() {
        let sql = "ALTER USER 'alice' SET PASSWORD 'SuperSecret123!'";
        let redacted = redact_sensitive_sql(sql);
        assert!(!redacted.contains("SuperSecret123"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_redact_create_user_password() {
        let sql = "CREATE USER bob PASSWORD 'mypassword'";
        let redacted = redact_sensitive_sql(sql);
        assert!(!redacted.contains("mypassword"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_redact_double_quotes() {
        let sql = r#"ALTER USER alice SET PASSWORD "mypassword""#;
        let redacted = redact_sensitive_sql(sql);
        assert!(!redacted.contains("mypassword"));
    }

    #[test]
    fn test_preserves_safe_queries() {
        let sql = "SELECT * FROM users WHERE name = 'alice'";
        let redacted = redact_sensitive_sql(sql);
        assert_eq!(sql, redacted);
    }

    #[test]
    fn test_preserves_queries_without_password() {
        let sql = "CREATE TABLE test (id INT)";
        let redacted = redact_sensitive_sql(sql);
        assert_eq!(sql, redacted);
    }

    #[test]
    fn test_case_insensitive() {
        let sql = "alter user alice set password 'secret'";
        let redacted = redact_sensitive_sql(sql);
        assert!(!redacted.contains("secret"));

        let sql = "ALTER USER alice SET PASSWORD 'secret'";
        let redacted = redact_sensitive_sql(sql);
        assert!(!redacted.contains("secret"));
    }
}
