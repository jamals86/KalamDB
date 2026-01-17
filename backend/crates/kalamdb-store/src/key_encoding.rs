//! Key encoding utilities for RocksDB keys.
//!
//! This module provides functions to encode and decode RocksDB keys for different
//! table types, ensuring consistent key formats across the codebase.

use anyhow::{Context, Result};

/// Encode a user table key: `{user_id}:{row_id}`
///
/// # Examples
///
/// ```
/// use kalamdb_store::key_encoding::user_key;
///
/// let key = user_key("user123", "msg001");
/// assert_eq!(key, "user123:msg001");
/// ```
pub fn user_key(user_id: &str, row_id: &str) -> String {
    // Pre-allocate capacity to avoid reallocation
    let mut s = String::with_capacity(user_id.len() + 1 + row_id.len());
    s.push_str(user_id);
    s.push(':');
    s.push_str(row_id);
    s
}

/// Parse a user table key into `(user_id, row_id)`
///
/// # Examples
///
/// ```
/// use kalamdb_store::key_encoding::parse_user_key;
///
/// let (user_id, row_id) = parse_user_key("user123:msg001").unwrap();
/// assert_eq!(user_id, "user123");
/// assert_eq!(row_id, "msg001");
/// ```
pub fn parse_user_key(key: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = key.splitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid user key format: {}", key);
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Encode a shared table key: `{row_id}`
///
/// # Examples
///
/// ```
/// use kalamdb_store::key_encoding::shared_key;
///
/// let key = shared_key("conv001");
/// assert_eq!(key, "conv001");
/// ```
pub fn shared_key(row_id: &str) -> String {
    row_id.to_string()
}

/// Encode a stream table key: `{timestamp_ms}:{row_id}`
///
/// # Examples
///
/// ```
/// use kalamdb_store::key_encoding::stream_key;
///
/// let key = stream_key(1697299200000, "evt001");
/// assert_eq!(key, "1697299200000:evt001");
/// ```
pub fn stream_key(timestamp_ms: i64, row_id: &str) -> String {
    // Pre-allocate: i64 max is 20 digits + ':' + row_id
    let mut s = String::with_capacity(21 + row_id.len());
    use std::fmt::Write;
    let _ = write!(s, "{}:{}", timestamp_ms, row_id);
    s
}

/// Parse a stream table key into `(timestamp_ms, row_id)`
///
/// # Examples
///
/// ```
/// use kalamdb_store::key_encoding::parse_stream_key;
///
/// let (ts, row_id) = parse_stream_key("1697299200000:evt001").unwrap();
/// assert_eq!(ts, 1697299200000);
/// assert_eq!(row_id, "evt001");
/// ```
pub fn parse_stream_key(key: &str) -> Result<(i64, String)> {
    let parts: Vec<&str> = key.splitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid stream key format: {}", key);
    }
    let timestamp_ms = parts[0].parse::<i64>().context("Failed to parse timestamp")?;
    Ok((timestamp_ms, parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_key_encoding() {
        let key = user_key("user123", "msg001");
        assert_eq!(key, "user123:msg001");

        let (user_id, row_id) = parse_user_key(&key).unwrap();
        assert_eq!(user_id, "user123");
        assert_eq!(row_id, "msg001");
    }

    #[test]
    fn test_user_key_with_colon_in_row_id() {
        let key = user_key("user123", "msg:001:abc");
        assert_eq!(key, "user123:msg:001:abc");

        let (user_id, row_id) = parse_user_key(&key).unwrap();
        assert_eq!(user_id, "user123");
        assert_eq!(row_id, "msg:001:abc");
    }

    #[test]
    fn test_shared_key_encoding() {
        let key = shared_key("conv001");
        assert_eq!(key, "conv001");
    }

    #[test]
    fn test_stream_key_encoding() {
        let key = stream_key(1697299200000, "evt001");
        assert_eq!(key, "1697299200000:evt001");

        let (ts, row_id) = parse_stream_key(&key).unwrap();
        assert_eq!(ts, 1697299200000);
        assert_eq!(row_id, "evt001");
    }

    #[test]
    fn test_invalid_user_key() {
        let result = parse_user_key("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_stream_key() {
        let result = parse_stream_key("not_a_number:evt001");
        assert!(result.is_err());
    }
}
