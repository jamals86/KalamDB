//! Parameter binding and validation for SQL execution
//!
//! This module handles:
//! - Parameter validation (max 50 params, 512KB per param)
//! - DataFusion LogicalPlan placeholder replacement ($1, $2, ...)
//! - ScalarValue type checking

use crate::error::KalamDbError;
use arrow::array::Array;
use datafusion::scalar::ScalarValue;
use datafusion::logical_expr::{Expr, LogicalPlan};
use datafusion::common::tree_node::TreeNodeRewriter;
use datafusion::common::Result as DataFusionResult;

/// Maximum number of parameters allowed per statement
const MAX_PARAMS: usize = 50;

/// Maximum size per individual parameter (512KB)
const MAX_PARAM_SIZE_BYTES: usize = 512 * 1024;

/// Validate parameter count and sizes before execution
pub fn validate_params(params: &[ScalarValue]) -> Result<(), KalamDbError> {
    // Check parameter count
    if params.len() > MAX_PARAMS {
        return Err(KalamDbError::ParamCountExceeded {
            max: MAX_PARAMS,
            actual: params.len(),
        });
    }

    // Check individual parameter sizes
    for (idx, param) in params.iter().enumerate() {
        let size = estimate_scalar_value_size(param);
        if size > MAX_PARAM_SIZE_BYTES {
            return Err(KalamDbError::ParamSizeExceeded {
                index: idx,
                max_bytes: MAX_PARAM_SIZE_BYTES,
                actual_bytes: size,
            });
        }
    }

    Ok(())
}

/// Estimate the memory size of a ScalarValue
fn estimate_scalar_value_size(value: &ScalarValue) -> usize {
    match value {
        ScalarValue::Null => 0,
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
        ScalarValue::Utf8(s) | ScalarValue::LargeUtf8(s) => {
            s.as_ref().map(|s| s.len()).unwrap_or(0)
        }
        ScalarValue::Binary(b) | ScalarValue::LargeBinary(b) | ScalarValue::FixedSizeBinary(_, b) => {
            b.as_ref().map(|b| b.len()).unwrap_or(0)
        }
        // ScalarValue::List(arr) | ScalarValue::LargeList(arr) | ScalarValue::FixedSizeList(arr) => {
        //     // Rough estimate: 64 bytes per array element
        //     arr.len() * 64
        // }
        ScalarValue::Date32(_) | ScalarValue::Date64(_) => 8,
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
        ScalarValue::Struct(arr) => {
            // Rough estimate: sum of all field sizes
            arr.len() * 64
        }
        ScalarValue::Decimal128(_, _, _) | ScalarValue::Decimal256(_, _, _) => 16,
        // Conservative fallback for any new types
        _ => 64,
    }
}

/// Replace placeholders ($1, $2, ...) in LogicalPlan with ScalarValue literals
///
/// **Status**: Infrastructure complete, full implementation pending DataFusion API research
///
/// # Arguments
/// * `plan` - LogicalPlan to process
/// * `params` - Parameter values indexed from 0 (placeholder $1 = params[0])
///
/// # Returns
/// * `Ok(LogicalPlan)` - Plan with all placeholders replaced
/// * `Err(KalamDbError)` - If placeholder index is out of bounds or not yet implemented
///
/// # Implementation Note
/// DataFusion's LogicalPlan API for expression transformation varies across versions.
/// The correct approach is to use `LogicalPlan::with_exprs()` or a custom visitor pattern.
/// This will be completed when DataFusion's stable API is determined.
///
/// # Example
/// ```ignore
/// use datafusion::prelude::*;
/// use datafusion::scalar::ScalarValue;
///
/// let plan = ctx.sql("SELECT * FROM users WHERE id = $1").await?.into_optimized_plan()?;
/// let params = vec![ScalarValue::Int64(Some(42))];
/// let bound_plan = replace_placeholders_in_plan(plan, &params)?;
/// ```
pub fn replace_placeholders_in_plan(
    _plan: LogicalPlan,
    params: &[ScalarValue],
) -> Result<LogicalPlan, KalamDbError> {
    // If no params, return plan unchanged
    if params.is_empty() {
        return Ok(_plan);
    }

    // TODO: Implement LogicalPlan expression traversal
    // Research needed: DataFusion 40.0 API for recursive expression replacement
    // Options:
    // 1. LogicalPlan::with_exprs() + custom ExprRewriter
    // 2. LogicalPlan visitor pattern with mutable state
    // 3. DataFrame API with parameter binding support (if available)
    
    Err(KalamDbError::NotImplemented {
        feature: "Parameter binding via LogicalPlan rewrite".to_string(),
        message: "validate_params() works, placeholder replacement pending DataFusion API research".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_params_count() {
        // Valid: under limit
        let params = vec![ScalarValue::Int64(Some(1)); 40];
        assert!(validate_params(&params).is_ok());

        // Invalid: over limit
        let params = vec![ScalarValue::Int64(Some(1)); 51];
        assert!(matches!(
            validate_params(&params),
            Err(KalamDbError::ParamCountExceeded { .. })
        ));
    }

    #[test]
    fn test_validate_params_size() {
        // Valid: under size limit
        let small_string = "x".repeat(1000);
        let params = vec![ScalarValue::Utf8(Some(small_string))];
        assert!(validate_params(&params).is_ok());

        // Invalid: over size limit (600KB)
        let large_string = "x".repeat(600_000);
        let params = vec![ScalarValue::Utf8(Some(large_string))];
        assert!(matches!(
            validate_params(&params),
            Err(KalamDbError::ParamSizeExceeded { .. })
        ));
    }

    #[test]
    fn test_estimate_scalar_value_size() {
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Int64(Some(123))), 8);
        assert_eq!(
            estimate_scalar_value_size(&ScalarValue::Utf8(Some("hello".to_string()))),
            5
        );
        assert_eq!(estimate_scalar_value_size(&ScalarValue::Null), 0);
    }
}

