//! Error types for kalamdb-live

use thiserror::Error;

/// Errors that can occur in live query operations
#[derive(Error, Debug, Clone)]
pub enum LiveError {
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Invalid SQL: {0}")]
    InvalidSql(String),

    #[error("System error: {0}")]
    System(String),

    #[error("Live query not found: {live_id}")]
    LiveQueryNotFound { live_id: String },

    #[error("Connection not found: {connection_id}")]
    ConnectionNotFound { connection_id: String },

    #[error("Invalid subscription parameter '{field}': {reason}")]
    InvalidSubscription { reason: String, field: String },

    #[error("Duplicate subscription ID '{subscription_id}' for connection '{connection_id}'")]
    DuplicateSubscription {
        subscription_id: String,
        connection_id: String,
    },

    #[error("Invalid query '{query}': {reason}")]
    InvalidQuery { query: String, reason: String },

    #[error("User '{user_id}' does not have access to table '{namespace}.{table}'")]
    TableAccessDenied {
        namespace: String,
        table: String,
        user_id: String,
    },

    #[error("Failed to compile filter '{filter}': {reason}")]
    FilterCompilationError { filter: String, reason: String },

    #[error("Connection '{connection_id}' has reached maximum subscriptions ({current}/{max})")]
    SubscriptionLimitExceeded {
        connection_id: String,
        current: usize,
        max: usize,
    },

    #[error("User '{user_id}' has reached maximum subscriptions ({current}/{max})")]
    UserSubscriptionLimitExceeded {
        user_id: String,
        current: usize,
        max: usize,
    },

    #[error("Other error: {0}")]
    Other(String),
}

impl From<serde_json::Error> for LiveError {
    fn from(e: serde_json::Error) -> Self {
        LiveError::SerializationError(e.to_string())
    }
}

impl From<datafusion::error::DataFusionError> for LiveError {
    fn from(e: datafusion::error::DataFusionError) -> Self {
        LiveError::InvalidSql(e.to_string())
    }
}

impl From<kalamdb_tables::TableError> for LiveError {
    fn from(e: kalamdb_tables::TableError) -> Self {
        LiveError::Storage(e.to_string())
    }
}

impl From<kalamdb_system::SystemError> for LiveError {
    fn from(e: kalamdb_system::SystemError) -> Self {
        LiveError::System(e.to_string())
    }
}

impl From<crate::schema_registry::error::RegistryError> for LiveError {
    fn from(e: crate::schema_registry::error::RegistryError) -> Self {
        LiveError::Storage(e.to_string())
    }
}

/// Result type for live query operations
pub type Result<T> = std::result::Result<T, LiveError>;

/// For backward compatibility with code expecting KalamDbError
pub type KalamDbError = LiveError;

/// Validation helper for subscription parameters
pub struct SubscriptionValidator;

impl SubscriptionValidator {
    /// Validate subscription ID format
    pub fn validate_subscription_id(subscription_id: &str) -> Result<()> {
        if subscription_id.is_empty() {
            return Err(LiveError::InvalidSubscription {
                reason: "subscription_id cannot be empty".to_string(),
                field: "subscription_id".to_string(),
            });
        }

        if subscription_id.len() > 128 {
            return Err(LiveError::InvalidSubscription {
                reason: format!(
                    "subscription_id too long ({} chars, max 128)",
                    subscription_id.len()
                ),
                field: "subscription_id".to_string(),
            });
        }

        // Only allow alphanumeric, dash, underscore
        if !subscription_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(LiveError::InvalidSubscription {
                reason: "subscription_id must contain only alphanumeric, dash, or underscore"
                    .to_string(),
                field: "subscription_id".to_string(),
            });
        }

        Ok(())
    }

    /// Validate query string
    pub fn validate_query(query: &str) -> Result<()> {
        if query.is_empty() {
            return Err(LiveError::InvalidSubscription {
                reason: "query cannot be empty".to_string(),
                field: "query".to_string(),
            });
        }

        if query.len() > 10_000 {
            return Err(LiveError::InvalidSubscription {
                reason: format!("query too long ({} chars, max 10000)", query.len()),
                field: "query".to_string(),
            });
        }

        // Must start with SELECT (case-insensitive)
        let trimmed = query.trim();
        if !trimmed.to_uppercase().starts_with("SELECT") {
            return Err(LiveError::InvalidQuery {
                query: query.to_string(),
                reason: "Live queries must be SELECT statements".to_string(),
            });
        }

        Ok(())
    }

    /// Validate table name format
    pub fn validate_table_name(table_name: &str) -> Result<()> {
        kalamdb_sql::validation::validate_table_name(table_name).map_err(|e| {
            LiveError::InvalidSubscription {
                reason: e.to_string(),
                field: "table_name".to_string(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_subscription_id() {
        // Valid
        assert!(SubscriptionValidator::validate_subscription_id("sub-123").is_ok());
        assert!(SubscriptionValidator::validate_subscription_id("my_subscription_1").is_ok());

        // Invalid - empty
        assert!(SubscriptionValidator::validate_subscription_id("").is_err());

        // Invalid - too long
        let long_id = "a".repeat(129);
        assert!(SubscriptionValidator::validate_subscription_id(&long_id).is_err());

        // Invalid - special characters
        assert!(SubscriptionValidator::validate_subscription_id("sub@123").is_err());
    }

    #[test]
    fn test_validate_query() {
        // Valid
        assert!(SubscriptionValidator::validate_query("SELECT * FROM users").is_ok());
        assert!(SubscriptionValidator::validate_query("  select id from events  ").is_ok());

        // Invalid - empty
        assert!(SubscriptionValidator::validate_query("").is_err());

        // Invalid - not SELECT
        assert!(SubscriptionValidator::validate_query("INSERT INTO users VALUES (1)").is_err());
        assert!(SubscriptionValidator::validate_query("DELETE FROM users").is_err());

        // Invalid - too long
        let long_query = format!("SELECT {}", "a".repeat(10001));
        assert!(SubscriptionValidator::validate_query(&long_query).is_err());
    }

    #[test]
    fn test_validate_table_name() {
        // Valid
        assert!(SubscriptionValidator::validate_table_name("users").is_ok());
        assert!(SubscriptionValidator::validate_table_name("my_table_123").is_ok());

        // Invalid - empty
        assert!(SubscriptionValidator::validate_table_name("").is_err());

        // Invalid - too long
        let long_name = "a".repeat(256);
        assert!(SubscriptionValidator::validate_table_name(&long_name).is_err());
    }

    #[test]
    fn test_error_messages() {
        let err = LiveError::LiveQueryNotFound {
            live_id: "test-123".to_string(),
        };
        assert_eq!(err.to_string(), "Live query not found: test-123");

        let err = LiveError::DuplicateSubscription {
            subscription_id: "sub-1".to_string(),
            connection_id: "conn-1".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Duplicate subscription ID 'sub-1' for connection 'conn-1'"
        );
    }
}
