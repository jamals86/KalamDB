//! Parameter validation for SQL execution
//!
//! Provides centralized validation for query parameters with configurable limits.

use crate::error::KalamDbError;
use crate::sql::executor::models::ScalarValue;
use kalamdb_configs::ExecutionSettings;

/// Parameter validation limits (from server.toml [execution] section)
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
    kalamdb_commons::estimate_scalar_value_size(value)
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
        let params: Vec<ScalarValue> = (0..51).map(|i| ScalarValue::Int32(Some(i))).collect();
        let limits = ParameterLimits::default();
        let result = validate_parameters(&params, &limits);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Parameter count exceeds limit"));
    }

    #[test]
    fn test_validate_parameters_size_exceeded() {
        let large_string = "a".repeat(600_000); // 600KB > 512KB limit
        let params = vec![ScalarValue::Utf8(Some(large_string))];
        let limits = ParameterLimits::default();
        let result = validate_parameters(&params, &limits);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("size exceeds limit"));
    }

    #[test]
    fn test_estimate_scalar_value_size() {
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Null), 1);
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Int32(Some(42))), 4);
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Utf8(Some("hello".to_string()))), 5);
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Float64(Some(3.25))), 8);
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
