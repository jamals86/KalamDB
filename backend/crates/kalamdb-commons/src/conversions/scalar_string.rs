//! String conversions for ScalarValue
//!
//! Provides conversion functions between ScalarValue and string representations.
//! Used for primary key handling, display formatting, and serialization.

use datafusion::scalar::ScalarValue;

/// Convert ScalarValue to string representation for primary keys
///
/// Converts various scalar types to their string representation suitable for
/// use as primary key values. Returns error for unsupported types.
///
/// # Supported Types
/// - Integers (Int8, Int16, Int32, Int64)
/// - Unsigned integers (UInt8, UInt16, UInt32, UInt64)
/// - Strings (Utf8, LargeUtf8)
/// - Booleans
///
/// # Example
///
/// ```rust,ignore
/// use kalamdb_commons::conversions::scalar_to_pk_string;
/// use datafusion::scalar::ScalarValue;
///
/// let value = ScalarValue::Int64(Some(12345));
/// assert_eq!(scalar_to_pk_string(&value).unwrap(), "12345");
///
/// let str_value = ScalarValue::Utf8(Some("user123".to_string()));
/// assert_eq!(scalar_to_pk_string(&str_value).unwrap(), "user123");
/// ```
pub fn scalar_to_pk_string(value: &ScalarValue) -> Result<String, String> {
    match value {
        ScalarValue::Int64(Some(n)) => Ok(n.to_string()),
        ScalarValue::Int32(Some(n)) => Ok(n.to_string()),
        ScalarValue::Int16(Some(n)) => Ok(n.to_string()),
        ScalarValue::Int8(Some(n)) => Ok(n.to_string()),
        ScalarValue::UInt64(Some(n)) => Ok(n.to_string()),
        ScalarValue::UInt32(Some(n)) => Ok(n.to_string()),
        ScalarValue::UInt16(Some(n)) => Ok(n.to_string()),
        ScalarValue::UInt8(Some(n)) => Ok(n.to_string()),
        ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => Ok(s.clone()),
        ScalarValue::Boolean(Some(b)) => Ok(b.to_string()),
        _ => Err(format!("Unsupported primary key type: {:?}", value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scalar_to_pk_string_int() {
        let value = ScalarValue::Int64(Some(12345));
        assert_eq!(scalar_to_pk_string(&value).unwrap(), "12345");
    }

    #[test]
    fn test_scalar_to_pk_string_string() {
        let value = ScalarValue::Utf8(Some("user123".to_string()));
        assert_eq!(scalar_to_pk_string(&value).unwrap(), "user123");
    }

    #[test]
    fn test_scalar_to_pk_string_bool() {
        let value = ScalarValue::Boolean(Some(true));
        assert_eq!(scalar_to_pk_string(&value).unwrap(), "true");
    }

    #[test]
    fn test_scalar_to_pk_string_null() {
        let value = ScalarValue::Int64(None);
        assert!(scalar_to_pk_string(&value).is_err());
    }
}
