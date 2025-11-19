//! WebSocket subscription models
//!
//! This module defines the structure for live query subscriptions sent via WebSocket.

use serde::{Deserialize, Serialize};

/// Subscription request for live query updates
///
/// Clients send this message to register for real-time notifications when data changes.
///
/// # Example
/// ```json
/// {
///   "id": "sub-1",
///   "sql": "SELECT * FROM messages WHERE user_id = CURRENT_USER() ORDER BY created_at DESC",
///   "options": {
///     "last_rows": 10
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Unique subscription identifier (client-generated)
    pub id: String,

    /// SQL query for live updates (must be a SELECT statement)
    pub sql: String,

    /// Optional subscription options
    #[serde(default)]
    pub options: SubscriptionOptions,
}

/// Options for live query subscriptions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscriptionOptions {
    /// Number of recent rows to fetch initially (default: 0, no initial fetch)
    #[serde(default)]
    pub last_rows: Option<usize>,

    /// Optional: Configure batch size for initial data streaming
    #[serde(default)]
    pub batch_size: Option<usize>,
}

/// Array of subscriptions sent during WebSocket connection
///
/// # Example
/// ```json
/// {
///   "subscriptions": [
///     {
///       "id": "sub-1",
///       "sql": "SELECT * FROM messages WHERE user_id = CURRENT_USER()",
///       "options": {"last_rows": 10}
///     },
///     {
///       "id": "sub-2",
///       "sql": "SELECT * FROM notifications WHERE user_id = CURRENT_USER()",
///       "options": {"last_rows": 5}
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRequest {
    /// List of subscriptions to register for this connection
    pub subscriptions: Vec<Subscription>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_serialization() {
        let sub = Subscription {
            id: "sub-1".to_string(),
            sql: "SELECT * FROM messages".to_string(),
            options: SubscriptionOptions {
                last_rows: Some(10),
                batch_size: None,
            },
        };

        let json = serde_json::to_string(&sub).unwrap();
        assert!(json.contains("sub-1"));
        assert!(json.contains("SELECT"));

        let deserialized: Subscription = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "sub-1");
        assert_eq!(deserialized.options.last_rows, Some(10));
    }

    #[test]
    fn test_subscription_request_with_multiple() {
        let request = SubscriptionRequest {
            subscriptions: vec![
                Subscription {
                    id: "sub-1".to_string(),
                    sql: "SELECT * FROM messages".to_string(),
                    options: SubscriptionOptions {
                        last_rows: Some(10),
                        batch_size: None,
                    },
                },
                Subscription {
                    id: "sub-2".to_string(),
                    sql: "SELECT * FROM notifications".to_string(),
                    options: SubscriptionOptions { 
                        last_rows: Some(5),
                        batch_size: None,
                    },
                },
            ],
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: SubscriptionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.subscriptions.len(), 2);
    }

    #[test]
    fn test_subscription_default_options() {
        let json = r#"{"id":"sub-1","sql":"SELECT * FROM users"}"#;
        let sub: Subscription = serde_json::from_str(json).unwrap();

        assert_eq!(sub.options.last_rows, None);
    }
}
