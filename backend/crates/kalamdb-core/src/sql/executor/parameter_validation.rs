//! Parameter validation for SQL execution
//!
//! Provides centralized validation for query parameters with configurable limits.

use crate::error::KalamDbError;
use crate::sql::executor::models::ScalarValue;
use kalamdb_commons::config::ExecutionSettings;

/// Parameter validation limits (from config.toml [execution] section)
pub struct ParameterLimits {
    /// Maximum number of parameters allowed (default: 50)
    pub max_count: usize,
    /// Maximum size of a single parameter in bytes (default: 512KB)
    pub max_size_bytes: usize,
}

impl Default for ParameterLimits {
    fn default() -> Self {
        Self {
            max_count: 50,
            max_size_bytes: 524288, // 512KB
        }
    }
}

impl ParameterLimits {
    /// Create ParameterLimits from ExecutionSettings config
    pub fn from_config(exec: &ExecutionSettings) -> Self {
        Self {
            max_count: exec.max_parameters,
            max_size_bytes: exec.max_parameter_size_bytes,
        }
    }
}

/// Validate parameters against configured limits
///
/// Checks:
/// - Parameter count does not exceed max_parameters
/// - Each parameter size does not exceed max_parameter_size_bytes
///
/// Returns error if any limit is exceeded.
pub fn validate_parameters(
    params: &[ScalarValue],
    limits: &ParameterLimits,
) -> Result<(), KalamDbError> {
    // Check parameter count (T024)
    if params.len() > limits.max_count {
        return Err(KalamDbError::InvalidOperation(format!(
            "Parameter count exceeds limit: {} > {} (max_parameters)",
            params.len(),
            limits.max_count
        )));
    }

    // Check parameter sizes (T024)
    for (idx, param) in params.iter().enumerate() {
        let size = estimate_scalar_value_size(param);
        if size > limits.max_size_bytes {
            return Err(KalamDbError::InvalidOperation(format!(
                "Parameter ${} size exceeds limit: {} bytes > {} bytes (max_parameter_size_bytes)",
                idx + 1,
                size,
                limits.max_size_bytes
            )));
        }
    }

    Ok(())
}

/// Estimate the size in bytes of a ScalarValue
///
/// Conservative estimation for memory safety.
fn estimate_scalar_value_size(value: &ScalarValue) -> usize {
    match value {
        ScalarValue::Null => 1,
        ScalarValue::Boolean(_) => 1,
        ScalarValue::Int8(_) => 1,
        ScalarValue::Int16(_) => 2,
        ScalarValue::Int32(_) => 4,
        ScalarValue::Int64(_) => 8,
        ScalarValue::UInt8(_) => 1,
        ScalarValue::UInt16(_) => 2,
        ScalarValue::UInt32(_) => 4,
        ScalarValue::UInt64(_) => 8,
        ScalarValue::Float32(_) => 4,
        ScalarValue::Float64(_) => 8,
        ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => s.len(),
        ScalarValue::Utf8(None) | ScalarValue::LargeUtf8(None) => 0,
        ScalarValue::Binary(Some(b)) | ScalarValue::LargeBinary(Some(b)) => b.len(),
        ScalarValue::Binary(None) | ScalarValue::LargeBinary(None) => 0,
        ScalarValue::FixedSizeBinary(_, Some(b)) => b.len(),
        ScalarValue::FixedSizeBinary(size, None) => *size as usize,
        ScalarValue::Date32(_) => 4,
        ScalarValue::Date64(_) => 8,
        ScalarValue::Time32Second(_) | ScalarValue::Time32Millisecond(_) => 4,
        ScalarValue::Time64Microsecond(_) | ScalarValue::Time64Nanosecond(_) => 8,
        ScalarValue::TimestampSecond(_, _)
        | ScalarValue::TimestampMillisecond(_, _)
        | ScalarValue::TimestampMicrosecond(_, _)
        | ScalarValue::TimestampNanosecond(_, _) => 8,
        ScalarValue::IntervalYearMonth(_) => 4,
        ScalarValue::IntervalDayTime(_) => 8,
        ScalarValue::IntervalMonthDayNano(_) => 16,
        ScalarValue::DurationSecond(_)
        | ScalarValue::DurationMillisecond(_)
        | ScalarValue::DurationMicrosecond(_)
        | ScalarValue::DurationNanosecond(_) => 8,
        ScalarValue::Decimal128(_, _, _) => 16,
        ScalarValue::Decimal256(_, _, _) => 32,
        // For complex types, estimate conservatively (assume 1KB per item)
        ScalarValue::List(_) => 1024,
        ScalarValue::LargeList(_) => 1024,
        ScalarValue::FixedSizeList(_) => 1024,
        ScalarValue::Struct(_) => 1024,
        ScalarValue::Dictionary(_, v) => estimate_scalar_value_size(v),
        _ => 8, // Default estimate for unknown types
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_parameters_within_limits() {
        let params = vec![
            ScalarValue::Int32(Some(42)),
            ScalarValue::Utf8(Some("test".to_string())),
        ];
        let limits = ParameterLimits::default();
        assert!(validate_parameters(&params, &limits).is_ok());
    }

    #[test]
    fn test_validate_parameters_count_exceeded() {
        let params: Vec<ScalarValue> = (0..51)
            .map(|i| ScalarValue::Int32(Some(i)))
            .collect();
        let limits = ParameterLimits::default();
        let result = validate_parameters(&params, &limits);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Parameter count exceeds limit"));
    }

    #[test]
    fn test_validate_parameters_size_exceeded() {
        let large_string = "a".repeat(600_000); // 600KB > 512KB limit
        let params = vec![ScalarValue::Utf8(Some(large_string))];
        let limits = ParameterLimits::default();
        let result = validate_parameters(&params, &limits);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("size exceeds limit"));
    }

    #[test]
    fn test_estimate_scalar_value_size() {
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Null), 1);
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Int32(Some(42))), 4);
        assert_eq!(
            estimate_scalar_value_size(&ScalarValue::Utf8(Some("hello".to_string()))),
            5
        );
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Float64(Some(3.14))), 8);
    }

    #[test]
    fn test_custom_limits() {
        let params = vec![ScalarValue::Int32(Some(1)); 10];
        let limits = ParameterLimits {
            max_count: 5,
            max_size_bytes: 1024,
        };
        let result = validate_parameters(&params, &limits);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Parameter count exceeds limit: 10 > 5"));
    }
}
