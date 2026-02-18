//! EXECUTE AS USER parsing utilities
//!
//! Provides a single shared implementation for parsing `EXECUTE AS USER` SQL
//! wrappers.  Both the API execution handler and the cluster forwarding logic
//! delegate to these functions so there is **one** canonical parser.
//!
//! Grammar:
//! ```text
//! EXECUTE AS USER <username> ( <inner_sql> )
//! ```
//! `<username>` may be single-quoted (`'alice'`) or bare (`alice`).
//! The inner SQL must be exactly one statement.

use crate::split_statements;

/// Prefix used for case-insensitive matching.
const EXECUTE_AS_PREFIX: &str = "EXECUTE AS USER";
/// Byte length of the prefix — used for fast slice comparisons.
const EXECUTE_AS_PREFIX_LEN: usize = EXECUTE_AS_PREFIX.len(); // 15

/// Result of parsing an `EXECUTE AS USER` envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteAsEnvelope {
    /// The username that follows `EXECUTE AS USER`.
    pub username: String, //TODO: Use UserName type 
    /// The inner SQL statement (parentheses stripped).
    pub inner_sql: String,
}

/// Check whether `sql` starts with `EXECUTE AS USER` (case-insensitive).
///
/// This is a very cheap check that avoids any allocation.
#[inline]
pub fn is_execute_as(sql: &str) -> bool {
    let trimmed = sql.trim();
    trimmed.len() >= EXECUTE_AS_PREFIX_LEN
        && trimmed.as_bytes()[..EXECUTE_AS_PREFIX_LEN]
            .eq_ignore_ascii_case(EXECUTE_AS_PREFIX.as_bytes())
}

/// Extract just the inner SQL from an `EXECUTE AS USER '...' (...)` wrapper.
///
/// Returns `Some(inner_sql)` if the SQL matches the EXECUTE AS USER pattern,
/// `None` otherwise.  This is intentionally lightweight — it only needs to
/// strip the wrapper so callers (e.g. statement classifier) can inspect the
/// actual SQL statement without allocating a full parsed result.
pub fn extract_inner_sql(sql: &str) -> Option<String> {
    let trimmed = sql.trim();
    if trimmed.len() < EXECUTE_AS_PREFIX_LEN
        || !trimmed.as_bytes()[..EXECUTE_AS_PREFIX_LEN]
            .eq_ignore_ascii_case(EXECUTE_AS_PREFIX.as_bytes())
    {
        return None;
    }

    // Find the opening parenthesis that wraps the inner SQL
    let open_paren = trimmed.find('(')?;
    // Find the matching closing parenthesis (last ')' in the string)
    let close_paren = trimmed.rfind(')')?;
    if close_paren <= open_paren + 1 {
        return None;
    }

    let inner = trimmed[open_paren + 1..close_paren].trim();
    if inner.is_empty() {
        return None;
    }

    Some(inner.to_string())
}

/// Fully parse an `EXECUTE AS USER` envelope, extracting the username and the
/// parenthesised inner SQL.
///
/// The username may be quoted (`'alice'`) or bare (`alice`).
/// Bare usernames extend up to the first whitespace or `(` character.
///
/// Returns:
/// - `Ok(Some(envelope))` when the statement is an EXECUTE AS USER wrapper
/// - `Ok(None)` when the statement is a normal SQL statement (no wrapper)
/// - `Err(msg)` on syntax errors inside the wrapper
pub fn parse_execute_as(statement: &str) -> Result<Option<ExecuteAsEnvelope>, String> {
    let trimmed = statement.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return Err("Empty SQL statement".to_string());
    }

    // Fast prefix check without allocating an uppercased copy.
    if trimmed.len() < EXECUTE_AS_PREFIX_LEN
        || !trimmed.as_bytes()[..EXECUTE_AS_PREFIX_LEN]
            .eq_ignore_ascii_case(EXECUTE_AS_PREFIX.as_bytes())
    {
        return Ok(None);
    }

    let after_prefix = trimmed[EXECUTE_AS_PREFIX_LEN..].trim_start();

    // --- Username extraction (quoted or bare) ---
    let (username, rest) = if after_prefix.starts_with('\'') {
        // Quoted: EXECUTE AS USER 'some name' (...)
        let after_quote = &after_prefix[1..];
        let end_quote = after_quote
            .find('\'')
            .ok_or_else(|| "EXECUTE AS USER username quote was not closed".to_string())?;
        let uname = after_quote[..end_quote].trim();
        if uname.is_empty() {
            return Err("EXECUTE AS USER username cannot be empty".to_string());
        }
        (uname, after_quote[end_quote + 1..].trim_start())
    } else {
        // Bare: EXECUTE AS USER alice (...)
        // Username extends until whitespace or '('.
        let end = after_prefix
            .find(|c: char| c.is_whitespace() || c == '(')
            .unwrap_or(after_prefix.len());
        let uname = after_prefix[..end].trim();
        if uname.is_empty() {
            return Err("EXECUTE AS USER username cannot be empty".to_string());
        }
        (uname, after_prefix[end..].trim_start())
    };

    // --- Parenthesised SQL body ---
    if !rest.starts_with('(') {
        return Err("EXECUTE AS USER must wrap SQL in parentheses".to_string());
    }

    let mut depth = 0usize;
    let mut close_idx = None;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Err("EXECUTE AS USER contains unbalanced parentheses".to_string());
                }
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(idx);
                    break;
                }
            },
            _ => {},
        }
    }

    let close_idx = close_idx.ok_or_else(|| "EXECUTE AS USER missing closing ')'".to_string())?;
    let inner_sql = rest[1..close_idx].trim();
    if inner_sql.is_empty() {
        return Err("EXECUTE AS USER requires a non-empty inner SQL statement".to_string());
    }

    let trailing = rest[close_idx + 1..].trim();
    if !trailing.is_empty() {
        return Err("EXECUTE AS USER must contain exactly one wrapped SQL statement".to_string());
    }

    let inner_statements = split_statements(inner_sql)
        .map_err(|e| format!("Failed to parse inner SQL for EXECUTE AS USER: {}", e))?;
    if inner_statements.len() != 1 {
        return Err("EXECUTE AS USER can only wrap a single SQL statement".to_string());
    }

    Ok(Some(ExecuteAsEnvelope {
        username: username.to_string(),
        inner_sql: inner_statements[0].trim().to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // is_execute_as
    // -----------------------------------------------------------------------

    #[test]
    fn is_execute_as_positive() {
        assert!(is_execute_as("EXECUTE AS USER 'alice' (SELECT 1)"));
        assert!(is_execute_as("execute as user bob (SELECT 1)"));
        assert!(is_execute_as("  EXECUTE AS USER 'x' (SELECT 1)  "));
    }

    #[test]
    fn is_execute_as_negative() {
        assert!(!is_execute_as("SELECT 1"));
        assert!(!is_execute_as("INSERT INTO t VALUES (1)"));
        assert!(!is_execute_as("EXECUTE SOMETHING ELSE"));
    }

    // -----------------------------------------------------------------------
    // extract_inner_sql
    // -----------------------------------------------------------------------

    #[test]
    fn extract_inner_sql_basic() {
        let inner = extract_inner_sql("EXECUTE AS USER 'alice' (SELECT * FROM t)");
        assert_eq!(inner.as_deref(), Some("SELECT * FROM t"));
    }

    #[test]
    fn extract_inner_sql_bare_username() {
        let inner = extract_inner_sql("EXECUTE AS USER bob (INSERT INTO t VALUES (1, 'x'))");
        assert_eq!(inner.as_deref(), Some("INSERT INTO t VALUES (1, 'x')"));
    }

    #[test]
    fn extract_inner_sql_non_wrapper() {
        assert!(extract_inner_sql("SELECT 1").is_none());
    }

    #[test]
    fn extract_inner_sql_case_insensitive() {
        let inner = extract_inner_sql("execute as user 'alice' (DELETE FROM t WHERE id = 1)");
        assert_eq!(inner.as_deref(), Some("DELETE FROM t WHERE id = 1"));
    }

    // -----------------------------------------------------------------------
    // parse_execute_as — quoted username
    // -----------------------------------------------------------------------

    #[test]
    fn parse_quoted_username() {
        let result = parse_execute_as(
            "EXECUTE AS USER 'alice' (SELECT * FROM default.todos WHERE id = 1);",
        )
        .expect("should parse")
        .expect("should be an envelope");

        assert_eq!(result.username, "alice");
        assert_eq!(result.inner_sql, "SELECT * FROM default.todos WHERE id = 1");
    }

    #[test]
    fn reject_multi_statement() {
        let err = parse_execute_as("EXECUTE AS USER 'alice' (SELECT 1; SELECT 2)")
            .expect_err("multiple statements should be rejected");
        assert!(err.contains("single SQL statement"));
    }

    // -----------------------------------------------------------------------
    // parse_execute_as — bare (unquoted) username
    // -----------------------------------------------------------------------

    #[test]
    fn parse_bare_username() {
        let result = parse_execute_as(
            "EXECUTE AS USER alice (SELECT * FROM default.todos WHERE id = 1);",
        )
        .expect("should parse")
        .expect("should be an envelope");

        assert_eq!(result.username, "alice");
        assert_eq!(result.inner_sql, "SELECT * FROM default.todos WHERE id = 1");
    }

    #[test]
    fn parse_bare_case_insensitive() {
        let result = parse_execute_as("execute as user bob (INSERT INTO default.t VALUES (1))")
            .expect("should parse")
            .expect("should be an envelope");

        assert_eq!(result.username, "bob");
        assert_eq!(result.inner_sql, "INSERT INTO default.t VALUES (1)");
    }

    #[test]
    fn parse_bare_no_space_before_paren() {
        let result = parse_execute_as("EXECUTE AS USER alice(SELECT 1)")
            .expect("should parse")
            .expect("should be an envelope");

        assert_eq!(result.username, "alice");
        assert_eq!(result.inner_sql, "SELECT 1");
    }

    // -----------------------------------------------------------------------
    // Passthrough (normal SQL)
    // -----------------------------------------------------------------------

    #[test]
    fn passthrough_normal_sql() {
        let result = parse_execute_as("SELECT * FROM default.todos WHERE id = 10")
            .expect("should parse");
        assert!(result.is_none());
    }
}
