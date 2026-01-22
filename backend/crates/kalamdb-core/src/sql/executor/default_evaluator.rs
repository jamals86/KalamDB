//! DEFAULT Value Evaluator
//!
//! Evaluates DEFAULT column values for INSERT and UPDATE operations.
//! Supports literal values and function calls (NOW, CURRENT_USER, SNOWFLAKE_ID, UUID_V7, ULID).

use crate::error::KalamDbError;
use crate::providers::arrow_json_conversion::json_value_to_scalar;
use kalamdb_system::SystemColumnsService;
use datafusion_common::ScalarValue;
use kalamdb_commons::models::UserId;
use kalamdb_commons::schemas::ColumnDefault;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Evaluate a DEFAULT column value
///
/// # Arguments
/// - `default`: The ColumnDefault specification from table schema
/// - `user_id`: Current user ID (for CURRENT_USER() function)
/// - `sys_cols`: Optional SystemColumnsService for SNOWFLAKE_ID generation (uses fresh generator if None)
///
/// # Returns
/// - `Ok(ScalarValue)`: Evaluated default value ready for Row construction
/// - `Err(KalamDbError)`: If function evaluation fails
pub fn evaluate_default(
    default: &ColumnDefault,
    user_id: &UserId,
    sys_cols: Option<Arc<SystemColumnsService>>,
) -> Result<ScalarValue, KalamDbError> {
    match default {
        ColumnDefault::None => {
            // No default - should not be called for this case
            Err(KalamDbError::InvalidOperation("Cannot evaluate None default".to_string()))
        },
        ColumnDefault::Literal(value) => {
            // Convert literal JSON to ScalarValue
            Ok(json_value_to_scalar(value))
        },
        ColumnDefault::FunctionCall { name, args } => {
            evaluate_function(name, args, user_id, sys_cols)
        },
    }
}

/// Evaluate a DEFAULT function call
fn evaluate_function(
    func_name: &str,
    args: &[JsonValue],
    user_id: &UserId,
    sys_cols: Option<Arc<SystemColumnsService>>,
) -> Result<ScalarValue, KalamDbError> {
    let func_upper = func_name.to_uppercase();

    match func_upper.as_str() {
        "CURRENT_USER" => {
            // Return current user ID as string
            if !args.is_empty() {
                return Err(KalamDbError::InvalidOperation(
                    "CURRENT_USER() takes no arguments".to_string(),
                ));
            }
            Ok(ScalarValue::Utf8(Some(user_id.as_str().to_string())))
        },

        "NOW" | "CURRENT_TIMESTAMP" => {
            // Return current timestamp in RFC3339 format (ISO 8601)
            if !args.is_empty() {
                return Err(KalamDbError::InvalidOperation(format!(
                    "{}() takes no arguments",
                    func_upper
                )));
            }
            let now = chrono::Utc::now();
            Ok(ScalarValue::Utf8(Some(now.to_rfc3339())))
        },

        "SNOWFLAKE_ID" => {
            // Generate Snowflake ID (64-bit sortable unique ID)
            if !args.is_empty() {
                return Err(KalamDbError::InvalidOperation(
                    "SNOWFLAKE_ID() takes no arguments".to_string(),
                ));
            }
            // Use SystemColumnsService for proper Snowflake ID generation
            match sys_cols {
                Some(svc) => {
                    let seq_id = svc.generate_seq_id().map_err(|e| {
                        KalamDbError::InvalidOperation(format!(
                            "Snowflake ID generation failed: {}",
                            e
                        ))
                    })?;
                    Ok(ScalarValue::Int64(Some(seq_id.as_i64())))
                },
                None => {
                    // Fallback: create fresh generator (non-singleton, for testing only)
                    use kalamdb_commons::ids::SnowflakeGenerator;
                    let generator = SnowflakeGenerator::new(0);
                    let id = generator.next_id().map_err(|e| {
                        KalamDbError::InvalidOperation(format!(
                            "Snowflake ID generation failed: {}",
                            e
                        ))
                    })?;
                    Ok(ScalarValue::Int64(Some(id)))
                },
            }
        },

        "UUID_V7" => {
            // Generate UUID v7 (time-ordered UUID)
            if !args.is_empty() {
                return Err(KalamDbError::InvalidOperation(
                    "UUID_V7() takes no arguments".to_string(),
                ));
            }
            use uuid::Uuid;
            let uuid = Uuid::now_v7();
            Ok(ScalarValue::Utf8(Some(uuid.to_string())))
        },

        "ULID" => {
            // Generate ULID (Universally Unique Lexicographically Sortable Identifier)
            if !args.is_empty() {
                return Err(KalamDbError::InvalidOperation(
                    "ULID() takes no arguments".to_string(),
                ));
            }
            // Use UUID v7 as a substitute for ULID (both are time-ordered)
            // KalamDB doesn't have a native ULID library dependency yet
            use uuid::Uuid;
            let uuid = Uuid::now_v7();
            // Convert to uppercase hex without dashes (ULID format)
            let ulid_like = uuid.to_string().replace('-', "").to_uppercase();
            Ok(ScalarValue::Utf8(Some(ulid_like)))
        },

        _ => Err(KalamDbError::InvalidOperation(format!(
            "Unknown DEFAULT function: {}",
            func_name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_literal() {
        let default = ColumnDefault::literal(serde_json::json!(42));
        let user_id = UserId::from("u_test123");
        let result = evaluate_default(&default, &user_id, None).unwrap();
        assert_eq!(result, ScalarValue::Int64(Some(42)));
    }

    #[test]
    fn test_evaluate_current_user() {
        let default = ColumnDefault::function("CURRENT_USER", vec![]);
        let user_id = UserId::from("u_test123");
        let result = evaluate_default(&default, &user_id, None).unwrap();
        assert_eq!(result, ScalarValue::Utf8(Some("u_test123".to_string())));
    }

    #[test]
    fn test_evaluate_now() {
        let default = ColumnDefault::function("NOW", vec![]);
        let user_id = UserId::from("u_test123");
        let result = evaluate_default(&default, &user_id, None).unwrap();
        // Should be a valid RFC3339 timestamp string
        if let ScalarValue::Utf8(Some(ts)) = result {
            assert!(chrono::DateTime::parse_from_rfc3339(&ts).is_ok());
        } else {
            panic!("NOW() default must return Utf8 scalar");
        }
    }

    #[test]
    fn test_evaluate_snowflake_id() {
        let default = ColumnDefault::function("SNOWFLAKE_ID", vec![]);
        let user_id = UserId::from("u_test123");
        let result = evaluate_default(&default, &user_id, None).unwrap();
        if let ScalarValue::Int64(Some(id)) = result {
            assert!(id > 0);
        } else {
            panic!("SNOWFLAKE_ID() default must return Int64 scalar");
        }
    }

    #[test]
    fn test_evaluate_uuid_v7() {
        let default = ColumnDefault::function("UUID_V7", vec![]);
        let user_id = UserId::from("u_test123");
        let result = evaluate_default(&default, &user_id, None).unwrap();
        if let ScalarValue::Utf8(Some(uuid_str)) = result {
            assert_eq!(uuid_str.len(), 36); // UUID format: 8-4-4-4-12
            assert!(uuid::Uuid::parse_str(&uuid_str).is_ok());
        } else {
            panic!("UUID_V7() default must return Utf8 scalar");
        }
    }

    #[test]
    fn test_evaluate_ulid() {
        let default = ColumnDefault::function("ULID", vec![]);
        let user_id = UserId::from("u_test123");
        let result = evaluate_default(&default, &user_id, None).unwrap();
        if let ScalarValue::Utf8(Some(ulid_str)) = result {
            assert_eq!(ulid_str.len(), 32); // 32 hex chars (UUID without dashes)
        } else {
            panic!("ULID() default must return Utf8 scalar");
        }
    }

    #[test]
    fn test_evaluate_current_user_with_args_fails() {
        let default = ColumnDefault::function("CURRENT_USER", vec![serde_json::json!("arg")]);
        let user_id = UserId::from("u_test123");
        let result = evaluate_default(&default, &user_id, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("takes no arguments"));
    }

    #[test]
    fn test_evaluate_unknown_function() {
        let default = ColumnDefault::function("UNKNOWN_FUNC", vec![]);
        let user_id = UserId::from("u_test123");
        let result = evaluate_default(&default, &user_id, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown DEFAULT function"));
    }
}
