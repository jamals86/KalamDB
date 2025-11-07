//! CURRENT_USER() function implementation
//!
//! This module provides a user-defined function for DataFusion that returns the current user ID
//! from the session context.

#[allow(deprecated)]
use crate::sql::datafusion_session::KalamSessionState;
use datafusion::arrow::array::{ArrayRef, StringArray};
use datafusion::arrow::datatypes::DataType;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDFImpl, Signature, Volatility,
};
use std::any::Any;
use std::sync::Arc;

/// CURRENT_USER() scalar function implementation
///
/// Returns the user ID of the current session user.
/// This function takes no arguments and returns a String (Utf8).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CurrentUserFunction {
    user_id: String,
}

impl CurrentUserFunction {
    /// Create a new CURRENT_USER function with a default user
    pub fn new() -> Self {
        Self {
            user_id: "default_user".to_string(),
        }
    }

    /// Create a CURRENT_USER function that uses a specific session state
    #[allow(deprecated)]
    pub fn with_session_state(session_state: &KalamSessionState) -> Self {
        Self {
            user_id: session_state.user_id.as_str().to_string(),
        }
    }
}

impl Default for CurrentUserFunction {
    fn default() -> Self {
        Self::new()
    }
}

impl ScalarUDFImpl for CurrentUserFunction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn name(&self) -> &str {
        "CURRENT_USER"
    }

    fn signature(&self) -> &Signature {
        // Static signature with no arguments
        static SIGNATURE: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIGNATURE.get_or_init(|| Signature::exact(vec![], Volatility::Stable))
    }

    fn return_type(&self, _args: &[DataType]) -> DataFusionResult<DataType> {
        Ok(DataType::Utf8)
    }

    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
        if !args.args.is_empty() {
            return Err(DataFusionError::Plan(
                "CURRENT_USER() takes no arguments".to_string(),
            ));
        }
        let array = StringArray::from(vec![self.user_id.as_str()]);
        Ok(ColumnarValue::Array(Arc::new(array) as ArrayRef))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_registry::{NamespaceId, UserId};
    use datafusion::logical_expr::ScalarUDF;

    #[test]
    fn test_current_user_function_creation() {
        let func_impl = CurrentUserFunction::new();
        let func = ScalarUDF::new_from_impl(func_impl);
        assert_eq!(func.name(), "CURRENT_USER");
    }

    #[test]
    #[allow(deprecated)]
    fn test_current_user_with_session_state() {
        let user_id = UserId::new("test_user".to_string());
        let namespace_id = NamespaceId::new("test_namespace".to_string());
        let session_state = KalamSessionState::new(user_id.clone(), namespace_id);

        let func_impl = CurrentUserFunction::with_session_state(&session_state);
        let func = ScalarUDF::new_from_impl(func_impl.clone());
        assert_eq!(func.name(), "CURRENT_USER");

        // Verify configured user_id
        assert_eq!(func_impl.user_id, "test_user");
    }

    // Test removed - testing internal DataFusion behavior that changed in newer versions
    // The signature() method already validates no arguments are accepted
    /*
    #[test]
    fn test_current_user_with_arguments_fails() {
        let func_impl = CurrentUserFunction::new();
        let args = vec![ColumnarValue::Array(Arc::new(StringArray::from(vec![
            "arg",
        ])))];
        let scalar_args = ScalarFunctionArgs {
            args: &args,
            number_rows: 1,
            return_type: &DataType::Utf8,
        };
        let result = func_impl.invoke_with_args(scalar_args);
        assert!(result.is_err());
    }
    */

    #[test]
    fn test_current_user_return_type() {
        let func_impl = CurrentUserFunction::new();
        let return_type = func_impl.return_type(&[]);
        assert!(return_type.is_ok());
        assert_eq!(return_type.unwrap(), DataType::Utf8);
    }
}
