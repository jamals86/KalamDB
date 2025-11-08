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
/// Note: This is a placeholder implementation. Full implementation requires:
/// 1. Traverse LogicalPlan recursively
/// 2. Find all Expr::Placeholder nodes
/// 3. Replace with Expr::Literal(params[placeholder_id - 1])
/// 4. Validate placeholder count matches params.len()
///
/// See DataFusion's PreparedStatement implementation for reference.
pub fn replace_placeholders_in_plan(
    _plan: LogicalPlan,
    params: &[ScalarValue],
) -> Result<LogicalPlan, KalamDbError> {
    // TODO: Implement LogicalPlan traversal and placeholder replacement
    // For now, return error indicating feature is not yet implemented
    if !params.is_empty() {
        return Err(KalamDbError::NotImplemented {
            feature: "Parameter binding via LogicalPlan rewrite".to_string(),
            message: "Parameter binding will be implemented in Phase 4 (US2)".to_string(),
        });
    }

    // If no params, return plan unchanged
    todo!("Implement placeholder replacement using TreeNode::rewrite and ExprRewriter")
}

/// Placeholder rewriter implementation
struct PlaceholderRewriter<'a> {
    params: &'a [ScalarValue],
}

impl<'a> TreeNodeRewriter for PlaceholderRewriter<'a> {
    type Node = Expr;

    fn f_down(&mut self, node: Self::Node) -> DataFusionResult<datafusion::common::tree_node::Transformed<Self::Node>> {
        // TODO: Implement actual rewrite logic
        // if let Expr::Placeholder { id, .. } = node {
        //     let param_idx = id.parse::<usize>()? - 1;
        //     return Ok(Transformed::yes(Expr::Literal(self.params[param_idx].clone())));
        // }
        Ok(datafusion::common::tree_node::Transformed::no(node))
    }
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
