//! Common parsing utilities
//!
//! This module provides shared parsing helpers to avoid code duplication across
//! custom parsers (CREATE STORAGE, FLUSH TABLE, KILL JOB, etc.).

/// Normalize SQL by removing extra whitespace and semicolons
///
/// Converts multiple spaces, tabs, newlines into single spaces and removes trailing semicolons.
///
/// # Examples
///
/// ```
/// use kalamdb_sql::parser::utils::normalize_sql;
///
/// let sql = "CREATE   STORAGE\n  s3_prod  ;";
/// assert_eq!(normalize_sql(sql), "CREATE STORAGE s3_prod");
/// ```
pub fn normalize_sql(sql: &str) -> String {
    sql.trim()
        .trim_end_matches(';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract a quoted string value after a keyword
///
/// Finds keyword (case-insensitive whole-word match) and extracts the quoted string after it.
///
/// # Examples
///
/// ```
/// use kalamdb_sql::parser::utils::extract_quoted_keyword_value;
///
/// let sql = "CREATE STORAGE s3 NAME 'Production Storage'";
/// let name = extract_quoted_keyword_value(sql, "NAME").unwrap();
/// assert_eq!(name, "Production Storage");
/// ```
///
/// # Errors
///
/// Returns error if:
/// - Keyword not found
/// - No quote after keyword
/// - Unclosed quote
pub fn extract_quoted_keyword_value(sql: &str, keyword: &str) -> Result<String, String> {
    let keyword_upper = keyword.to_uppercase();
    let sql_upper = sql.to_uppercase();

    // Find keyword as a whole word (surrounded by whitespace or start/end)
    let keyword_pos = find_whole_word(&sql_upper, &keyword_upper)
        .ok_or_else(|| format!("{} keyword not found", keyword))?;

    let after_keyword = &sql[keyword_pos + keyword.len()..];

    // Find the quoted value
    let quote_start = after_keyword
        .find('\'')
        .ok_or_else(|| format!("Missing quoted value after {}", keyword))?;

    let after_quote = &after_keyword[quote_start + 1..];
    let quote_end = after_quote
        .find('\'')
        .ok_or_else(|| format!("Unclosed quote after {}", keyword))?;

    Ok(after_quote[..quote_end].to_string())
}

/// Extract an unquoted keyword value (allows both quoted and unquoted)
///
/// Similar to `extract_quoted_keyword_value` but also accepts unquoted values.
///
/// # Examples
///
/// ```
/// use kalamdb_sql::parser::utils::extract_keyword_value;
///
/// // Quoted value
/// let sql = "CREATE STORAGE s3 TYPE 's3' NAME 'Prod'";
/// assert_eq!(extract_keyword_value(sql, "TYPE").unwrap(), "s3");
///
/// // Unquoted value
/// let sql2 = "CREATE STORAGE s3 TYPE filesystem";
/// assert_eq!(extract_keyword_value(sql2, "TYPE").unwrap(), "filesystem");
/// ```
pub fn extract_keyword_value(sql: &str, keyword: &str) -> Result<String, String> {
    let keyword_upper = keyword.to_uppercase();
    let sql_upper = sql.to_uppercase();

    let keyword_pos = find_whole_word(&sql_upper, &keyword_upper)
        .ok_or_else(|| format!("{} keyword not found", keyword))?;

    let after_keyword = &sql[keyword_pos + keyword.len()..];
    let trimmed = after_keyword.trim();

    if let Some(after_quote) = trimmed.strip_prefix('\'') {
        // Quoted value
        let quote_end = after_quote
            .find('\'')
            .ok_or_else(|| format!("Unclosed quote after {}", keyword))?;
        Ok(after_quote[..quote_end].to_string())
    } else {
        // Unquoted value - extract next whitespace-separated token
        let value = trimmed
            .split_whitespace()
            .next()
            .ok_or_else(|| format!("Missing value after {}", keyword))?;
        Ok(value.to_string())
    }
}

/// Extract a storage/table/namespace identifier after N tokens
///
/// Skips N tokens and extracts the next identifier.
///
/// # Examples
///
/// ```
/// use kalamdb_sql::parser::utils::extract_identifier;
///
/// let sql = "CREATE STORAGE s3_prod";
/// let id = extract_identifier(sql, 2).unwrap(); // Skip "CREATE" and "STORAGE"
/// assert_eq!(id, "s3_prod");
/// ```
pub fn extract_identifier(sql: &str, skip_tokens: usize) -> Result<String, String> {
    let parts: Vec<&str> = sql.split_whitespace().collect();
    if parts.len() <= skip_tokens {
        return Err(format!("Expected identifier after {} tokens", skip_tokens));
    }
    Ok(parts[skip_tokens].to_string())
}

/// Extract a qualified table name (namespace.table_name)
///
/// Parses a fully qualified table reference and returns (namespace, table_name).
///
/// # Examples
///
/// ```
/// use kalamdb_sql::parser::utils::extract_qualified_table;
///
/// let (ns, table) = extract_qualified_table("prod.events").unwrap();
/// assert_eq!(ns, "prod");
/// assert_eq!(table, "events");
/// ```
///
/// # Errors
///
/// Returns error if:
/// - Table reference is not qualified (missing namespace)
/// - Invalid format
pub fn extract_qualified_table(table_ref: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = table_ref.split('.').collect();
    if parts.len() != 2 {
        return Err("Table name must be qualified (namespace.table_name)".to_string());
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Find whole-word match in a string (case-insensitive)
///
/// Returns the starting position of the keyword if found as a complete word.
/// A complete word is surrounded by whitespace or start/end of string.
fn find_whole_word(haystack: &str, needle: &str) -> Option<usize> {
    let mut search_pos = 0;
    loop {
        match haystack[search_pos..].find(needle) {
            Some(pos) => {
                let absolute_pos = search_pos + pos;
                let is_word_start = absolute_pos == 0
                    || haystack
                        .chars()
                        .nth(absolute_pos - 1)
                        .unwrap()
                        .is_whitespace();
                let is_word_end = absolute_pos + needle.len() >= haystack.len()
                    || haystack
                        .chars()
                        .nth(absolute_pos + needle.len())
                        .unwrap()
                        .is_whitespace();

                if is_word_start && is_word_end {
                    return Some(absolute_pos);
                } else {
                    search_pos = absolute_pos + 1;
                }
            }
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_sql() {
        assert_eq!(normalize_sql("  SELECT  * ;"), "SELECT *");
        assert_eq!(
            normalize_sql("CREATE\n  STORAGE\n  s3  "),
            "CREATE STORAGE s3"
        );
    }

    #[test]
    fn test_extract_quoted_keyword_value() {
        let sql = "CREATE STORAGE s3 NAME 'Production Storage'";
        assert_eq!(
            extract_quoted_keyword_value(sql, "NAME").unwrap(),
            "Production Storage"
        );

        // Keyword not found
        assert!(extract_quoted_keyword_value(sql, "MISSING").is_err());

        // Missing quote
        let bad_sql = "NAME value";
        assert!(extract_quoted_keyword_value(bad_sql, "NAME").is_err());
    }

    #[test]
    fn test_extract_keyword_value() {
        // Quoted
        let sql = "TYPE 's3'";
        assert_eq!(extract_keyword_value(sql, "TYPE").unwrap(), "s3");

        // Unquoted
        let sql2 = "TYPE filesystem";
        assert_eq!(extract_keyword_value(sql2, "TYPE").unwrap(), "filesystem");
    }

    #[test]
    fn test_extract_identifier() {
        let sql = "CREATE STORAGE s3_prod";
        assert_eq!(extract_identifier(sql, 2).unwrap(), "s3_prod");

        // Not enough tokens
        assert!(extract_identifier(sql, 10).is_err());
    }

    #[test]
    fn test_extract_qualified_table() {
        let (ns, table) = extract_qualified_table("prod.events").unwrap();
        assert_eq!(ns, "prod");
        assert_eq!(table, "events");

        // Not qualified
        assert!(extract_qualified_table("events").is_err());
        assert!(extract_qualified_table("a.b.c").is_err());
    }

    #[test]
    fn test_find_whole_word() {
        let text = "CREATE TABLE tables IN namespace";
        assert_eq!(find_whole_word(&text.to_uppercase(), "TABLE"), Some(7));
        assert_eq!(find_whole_word(&text.to_uppercase(), "TABLES"), Some(13));
        assert_eq!(find_whole_word(&text.to_uppercase(), "IN"), Some(20));

        // Not a whole word
        assert!(find_whole_word("TABLES", "TABLE").is_none());
    }
}
