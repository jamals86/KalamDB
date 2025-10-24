//! UUID_V7() function implementation
//!
//! This module provides a user-defined function for DataFusion that generates
//! UUIDv7 identifiers following RFC 9562.
//!
//! UUIDv7 format:
//! - 48 bits: Unix timestamp in milliseconds
//! - 12 bits: randomized version and variant bits  
//! - 62 bits: random data
//!
//! UUIDv7 provides time-ordered UUIDs suitable for PRIMARY KEY columns
//! while maintaining global uniqueness.

use datafusion::arrow::array::{ArrayRef, StringArray};
use datafusion::arrow::datatypes::DataType;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{ColumnarValue, ScalarUDFImpl, Signature, Volatility};
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;

/// UUID_V7() scalar function implementation
///
/// Generates a 128-bit globally unique identifier following RFC 9562 UUIDv7 specification.
/// The UUID includes a 48-bit timestamp for time-based ordering.
///
/// # Returns
/// - STRING (Utf8) - A UUIDv7 in standard hyphenated format (8-4-4-4-12)
///
/// # Properties
/// - VOLATILE: Generates a new UUID on each invocation
/// - Time-ordered: UUIDs increase monotonically with time
/// - RFC 9562 compliant: Follows UUIDv7 specification
/// - Globally unique: Extremely low collision probability
#[derive(Debug, Clone)]
pub struct UuidV7Function;

impl UuidV7Function {
    /// Create a new UUID_V7 function
    pub fn new() -> Self {
        Self
    }

    /// Generate a single UUIDv7
    fn generate_uuid(&self) -> String {
        // Use the uuid crate's v7 implementation
        let uuid = Uuid::now_v7();
        uuid.to_string()
    }
}

impl Default for UuidV7Function {
    fn default() -> Self {
        Self::new()
    }
}

impl ScalarUDFImpl for UuidV7Function {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn name(&self) -> &str {
        "UUID_V7"
    }

    fn signature(&self) -> &Signature {
        // Static signature with no arguments
        static SIGNATURE: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIGNATURE.get_or_init(|| Signature::exact(vec![], Volatility::Volatile))
    }

    fn return_type(&self, _args: &[DataType]) -> DataFusionResult<DataType> {
        Ok(DataType::Utf8)
    }

    fn invoke(&self, args: &[ColumnarValue]) -> DataFusionResult<ColumnarValue> {
        if !args.is_empty() {
            return Err(DataFusionError::Plan(
                "UUID_V7() takes no arguments".to_string(),
            ));
        }

        // Generate a single UUID
        let uuid_str = self.generate_uuid();
        let array = StringArray::from(vec![uuid_str.as_str()]);
        Ok(ColumnarValue::Array(Arc::new(array) as ArrayRef))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::array::Array;
    use datafusion::logical_expr::ScalarUDF;
    use std::collections::HashSet;

    #[test]
    fn test_uuid_v7_function_creation() {
        let func_impl = UuidV7Function::new();
        let func = ScalarUDF::new_from_impl(func_impl);
        assert_eq!(func.name(), "UUID_V7");
    }

    #[test]
    fn test_uuid_v7_generation() {
        let func_impl = UuidV7Function::new();
        let uuid1 = func_impl.generate_uuid();
        let uuid2 = func_impl.generate_uuid();

        // Verify format (8-4-4-4-12)
        assert_eq!(uuid1.len(), 36);
        assert_eq!(uuid1.chars().filter(|&c| c == '-').count(), 4);

        // Verify uniqueness
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_uuid_v7_format_compliance() {
        let func_impl = UuidV7Function::new();
        let uuid_str = func_impl.generate_uuid();

        // RFC 9562 format: 8-4-4-4-12 (36 characters total with hyphens)
        assert_eq!(uuid_str.len(), 36, "UUID should be 36 characters");

        // Verify hyphen positions
        assert_eq!(&uuid_str[8..9], "-", "Expected hyphen at position 8");
        assert_eq!(&uuid_str[13..14], "-", "Expected hyphen at position 13");
        assert_eq!(&uuid_str[18..19], "-", "Expected hyphen at position 18");
        assert_eq!(&uuid_str[23..24], "-", "Expected hyphen at position 23");

        // Verify version 7 (14th character should be '7')
        assert_eq!(&uuid_str[14..15], "7", "UUIDv7 version bit should be 7");

        // Verify variant (19th character should be 8, 9, a, or b)
        let variant_char = uuid_str.chars().nth(19).unwrap();
        assert!(
            matches!(variant_char, '8' | '9' | 'a' | 'b' | 'A' | 'B'),
            "Variant bits should be 10xx (hex 8-b), got: {}",
            variant_char
        );
    }

    #[test]
    fn test_uuid_v7_uniqueness() {
        let func_impl = UuidV7Function::new();
        let mut uuids = HashSet::new();

        // Generate 10000 UUIDs and ensure no duplicates
        for _ in 0..10000 {
            let uuid = func_impl.generate_uuid();
            assert!(
                uuids.insert(uuid.clone()),
                "Duplicate UUID detected: {}",
                uuid
            );
        }
    }

    #[test]
    fn test_uuid_v7_time_ordering() {
        let func_impl = UuidV7Function::new();
        let uuid1 = func_impl.generate_uuid();

        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(2));

        let uuid2 = func_impl.generate_uuid();

        // UUIDv7 should be lexicographically ordered by time
        // (timestamp is in the first 48 bits)
        assert!(
            uuid1 < uuid2,
            "UUIDv7 should be time-ordered: {} < {}",
            uuid1,
            uuid2
        );
    }

    #[test]
    fn test_uuid_v7_invoke() {
        let func_impl = UuidV7Function::new();
        let result = func_impl.invoke(&[]);
        assert!(result.is_ok());

        if let Ok(ColumnarValue::Array(arr)) = result {
            let string_array = arr.as_any().downcast_ref::<StringArray>().unwrap();
            assert_eq!(string_array.len(), 1);
            let uuid_str = string_array.value(0);
            assert_eq!(uuid_str.len(), 36);
        } else {
            panic!("Expected array result");
        }
    }

    #[test]
    fn test_uuid_v7_with_arguments_fails() {
        let func_impl = UuidV7Function::new();
        let args = vec![ColumnarValue::Array(Arc::new(StringArray::from(vec![
            "arg",
        ])))];
        let result = func_impl.invoke(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_uuid_v7_return_type() {
        let func_impl = UuidV7Function::new();
        let return_type = func_impl.return_type(&[]);
        assert!(return_type.is_ok());
        assert_eq!(return_type.unwrap(), DataType::Utf8);
    }
}
