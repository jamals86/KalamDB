//! CURRENT_USER() function implementation
//!
//! This module provides a user-defined function for DataFusion that returns the current user ID
//! from the session context.

use crate::catalog::UserId;
use crate::sql::datafusion_session::KalamSessionState;
use datafusion::arrow::array::{ArrayRef, StringArray};
use datafusion::arrow::datatypes::DataType;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{ScalarUDF, Signature, TypeSignature, Volatility};
use datafusion::physical_plan::ColumnarValue;
use std::sync::Arc;

/// CURRENT_USER() scalar function
///
/// Returns the user ID of the current session user.
/// This function takes no arguments and returns a String (Utf8).
pub struct CurrentUserFunction;

impl CurrentUserFunction {
    /// Create a new CURRENT_USER function
    pub fn new() -> ScalarUDF {
        let signature = Signature::new(
            TypeSignature::Exact(vec![]),
            Volatility::Stable,
        );

        ScalarUDF::new(
            "CURRENT_USER",
            &signature,
            &DataType::Utf8,
            &Self::invoke,
        )
    }

    /// Invoke the CURRENT_USER function
    fn invoke(args: &[ColumnarValue]) -> DataFusionResult<ColumnarValue> {
        if !args.is_empty() {
            return Err(DataFusionError::Plan(
                "CURRENT_USER() takes no arguments".to_string(),
            ));
        }

        // TODO: Extract user_id from session context
        // For now, return a placeholder value
        // This will be properly implemented when session context is passed through
        let user_id = "default_user";
        
        let array = StringArray::from(vec![user_id]);
        Ok(ColumnarValue::Array(Arc::new(array) as ArrayRef))
    }

    /// Create a CURRENT_USER function that uses a specific session state
    pub fn with_session_state(session_state: &KalamSessionState) -> ScalarUDF {
        let user_id = session_state.user_id.clone();
        
        let signature = Signature::new(
            TypeSignature::Exact(vec![]),
            Volatility::Stable,
        );

        ScalarUDF::new(
            "CURRENT_USER",
            &signature,
            &DataType::Utf8,
            &move |args: &[ColumnarValue]| {
                if !args.is_empty() {
                    return Err(DataFusionError::Plan(
                        "CURRENT_USER() takes no arguments".to_string(),
                    ));
                }

                let array = StringArray::from(vec![user_id.as_ref().as_str()]);
                Ok(ColumnarValue::Array(Arc::new(array) as ArrayRef))
            },
        )
    }
}

impl Default for CurrentUserFunction {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{NamespaceId, TableCache};

    #[test]
    fn test_current_user_function_creation() {
        let func = CurrentUserFunction::new();
        assert_eq!(func.name(), "CURRENT_USER");
    }

    #[test]
    fn test_current_user_with_session_state() {
        let user_id = UserId::new("test_user".to_string());
        let namespace_id = NamespaceId::new("test_namespace".to_string());
        let table_cache = TableCache::new();
        let session_state = KalamSessionState::new(user_id.clone(), namespace_id, table_cache);
        
        let func = CurrentUserFunction::with_session_state(&session_state);
        assert_eq!(func.name(), "CURRENT_USER");
        
        // Test invocation
        let result = (func.fun())(&[]);
        assert!(result.is_ok());
        
        if let Ok(ColumnarValue::Array(arr)) = result {
            let string_array = arr.as_any().downcast_ref::<StringArray>().unwrap();
            assert_eq!(string_array.len(), 1);
            assert_eq!(string_array.value(0), "test_user");
        } else {
            panic!("Expected array result");
        }
    }

    #[test]
    fn test_current_user_with_arguments_fails() {
        let func = CurrentUserFunction::new();
        let args = vec![ColumnarValue::Array(Arc::new(StringArray::from(vec!["arg"])))];
        let result = (func.fun())(&args);
        assert!(result.is_err());
    }
}
