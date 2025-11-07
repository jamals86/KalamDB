//! Structured error response format for API responses

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::error::KalamDbError;

/// Structured error response with machine-readable code and context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Machine-readable error code (UPPERCASE_SNAKE_CASE)
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Context-specific error details (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Request ID for tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Error timestamp
    pub timestamp: DateTime<Utc>,
}

impl ErrorResponse {
    /// Create a new error response
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
        request_id: Option<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details,
            request_id,
            timestamp: Utc::now(),
        }
    }

    /// Create from KalamDbError with request ID
    pub fn from_error(err: &KalamDbError, request_id: Option<String>) -> Self {
        let (code, message, details) = match err {
            KalamDbError::ParamCountExceeded { max, actual } => (
                "PARAM_COUNT_EXCEEDED",
                format!("Parameter count exceeded: maximum {} parameters allowed, got {}", max, actual),
                Some(serde_json::json!({
                    "max": max,
                    "actual": actual,
                })),
            ),
            KalamDbError::ParamSizeExceeded { index, max_bytes, actual_bytes } => (
                "PARAM_SIZE_EXCEEDED",
                format!(
                    "Parameter size exceeded: parameter at index {} is {} bytes (max {} bytes)",
                    index, actual_bytes, max_bytes
                ),
                Some(serde_json::json!({
                    "index": index,
                    "max_bytes": max_bytes,
                    "actual_bytes": actual_bytes,
                })),
            ),
            KalamDbError::ParamCountMismatch { expected, actual } => (
                "PARAM_COUNT_MISMATCH",
                format!("Parameter count mismatch: expected {} parameters, got {}", expected, actual),
                Some(serde_json::json!({
                    "expected": expected,
                    "actual": actual,
                })),
            ),
            KalamDbError::ParamsNotSupported { statement_type } => (
                "PARAMS_NOT_SUPPORTED",
                format!("Parameters are not supported for {} statements", statement_type),
                Some(serde_json::json!({
                    "statement_type": statement_type,
                    "supported_types": ["SELECT", "INSERT", "UPDATE", "DELETE"],
                })),
            ),
            KalamDbError::Timeout { timeout_seconds } => (
                "TIMEOUT_HANDLER_EXECUTION",
                format!("Handler execution exceeded {}s timeout", timeout_seconds),
                Some(serde_json::json!({
                    "timeout_seconds": timeout_seconds,
                })),
            ),
            KalamDbError::PermissionDenied(msg) | KalamDbError::Unauthorized(msg) => (
                "AUTH_INSUFFICIENT_ROLE",
                msg.clone(),
                None,
            ),
            KalamDbError::TableNotFound(msg) => (
                "NOT_FOUND_TABLE",
                msg.clone(),
                None,
            ),
            KalamDbError::NamespaceNotFound(msg) => (
                "NOT_FOUND_NAMESPACE",
                msg.clone(),
                None,
            ),
            KalamDbError::NotImplemented { feature, message } => (
                "NOT_IMPLEMENTED",
                format!("{}: {}", feature, message),
                Some(serde_json::json!({
                    "feature": feature,
                    "message": message,
                })),
            ),
            KalamDbError::InvalidSql(msg) => (
                "INVALID_SQL_SYNTAX",
                msg.clone(),
                None,
            ),
            _ => (
                "INTERNAL_ERROR",
                err.to_string(),
                None,
            ),
        };

        Self::new(code, message, details, request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_response_from_param_count_exceeded() {
        let err = KalamDbError::ParamCountExceeded { max: 50, actual: 51 };
        let response = ErrorResponse::from_error(&err, Some("req_123".to_string()));
        
        assert_eq!(response.code, "PARAM_COUNT_EXCEEDED");
        assert!(response.message.contains("50"));
        assert!(response.message.contains("51"));
        assert_eq!(response.request_id, Some("req_123".to_string()));
        assert!(response.details.is_some());
    }

    #[test]
    fn test_error_response_from_param_size_exceeded() {
        let err = KalamDbError::ParamSizeExceeded {
            index: 0,
            max_bytes: 524288,
            actual_bytes: 600000,
        };
        let response = ErrorResponse::from_error(&err, None);
        
        assert_eq!(response.code, "PARAM_SIZE_EXCEEDED");
        assert!(response.message.contains("600000"));
        assert!(response.message.contains("524288"));
        assert!(response.details.is_some());
    }
}
