use serde_json::Value as JsonValue;

/// Parse an i64 from a JSON value that might be a Number or a String.
///
/// The backend serializes Int64 as strings to preserve precision in JSON.
/// This utility handles both formats for convenience.
///
/// # Example
///
/// ```rust
/// use serde_json::json;
/// use kalam_link::parse_i64;
///
/// let num_value = json!(42);
/// let str_value = json!("42");
///
/// assert_eq!(parse_i64(&num_value), 42);
/// assert_eq!(parse_i64(&str_value), 42);
/// ```
pub fn parse_i64(value: &JsonValue) -> i64 {
    match value {
        JsonValue::Number(n) => n.as_i64().unwrap_or(0),
        JsonValue::String(s) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}
