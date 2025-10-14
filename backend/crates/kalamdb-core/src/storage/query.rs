use serde::{Deserialize, Serialize};

/// Parameters for querying messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParams {
    /// Filter by conversation ID
    pub conversation_id: Option<String>,
    
    /// Return messages after this message ID (exclusive)
    pub since_msg_id: Option<u64>,
    
    /// Maximum number of messages to return
    pub limit: Option<usize>,
    
    /// Sort order: "asc" or "desc" (default: "asc")
    pub order: Option<String>,
}

impl QueryParams {
    /// Validate query parameters
    pub fn validate(&self) -> Result<(), String> {
        // Validate limit
        if let Some(limit) = self.limit {
            if limit == 0 {
                return Err("limit must be greater than 0".to_string());
            }
            if limit > 1000 {
                return Err("limit cannot exceed 1000".to_string());
            }
        }
        
        // Validate order
        if let Some(ref order) = self.order {
            if order != "asc" && order != "desc" {
                return Err("order must be 'asc' or 'desc'".to_string());
            }
        }
        
        Ok(())
    }
    
    /// Get the effective limit (default to 50 if not specified)
    pub fn effective_limit(&self) -> usize {
        self.limit.unwrap_or(50)
    }
    
    /// Get the effective order (default to "asc" if not specified)
    pub fn effective_order(&self) -> &str {
        self.order.as_deref().unwrap_or("asc")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_params_validation_valid() {
        let params = QueryParams {
            conversation_id: Some("conv_123".to_string()),
            since_msg_id: Some(100),
            limit: Some(50),
            order: Some("asc".to_string()),
        };
        
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_query_params_validation_valid_desc() {
        let params = QueryParams {
            conversation_id: None,
            since_msg_id: None,
            limit: Some(100),
            order: Some("desc".to_string()),
        };
        
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_query_params_validation_zero_limit() {
        let params = QueryParams {
            conversation_id: Some("conv_123".to_string()),
            since_msg_id: None,
            limit: Some(0),
            order: None,
        };
        
        let result = params.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "limit must be greater than 0");
    }

    #[test]
    fn test_query_params_validation_excessive_limit() {
        let params = QueryParams {
            conversation_id: Some("conv_123".to_string()),
            since_msg_id: None,
            limit: Some(1001),
            order: None,
        };
        
        let result = params.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "limit cannot exceed 1000");
    }

    #[test]
    fn test_query_params_validation_invalid_order() {
        let params = QueryParams {
            conversation_id: Some("conv_123".to_string()),
            since_msg_id: None,
            limit: Some(50),
            order: Some("invalid".to_string()),
        };
        
        let result = params.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "order must be 'asc' or 'desc'");
    }

    #[test]
    fn test_query_params_effective_limit_default() {
        let params = QueryParams {
            conversation_id: None,
            since_msg_id: None,
            limit: None,
            order: None,
        };
        
        assert_eq!(params.effective_limit(), 50);
    }

    #[test]
    fn test_query_params_effective_limit_specified() {
        let params = QueryParams {
            conversation_id: None,
            since_msg_id: None,
            limit: Some(100),
            order: None,
        };
        
        assert_eq!(params.effective_limit(), 100);
    }

    #[test]
    fn test_query_params_effective_order_default() {
        let params = QueryParams {
            conversation_id: None,
            since_msg_id: None,
            limit: None,
            order: None,
        };
        
        assert_eq!(params.effective_order(), "asc");
    }

    #[test]
    fn test_query_params_effective_order_specified() {
        let params = QueryParams {
            conversation_id: None,
            since_msg_id: None,
            limit: None,
            order: Some("desc".to_string()),
        };
        
        assert_eq!(params.effective_order(), "desc");
    }

    #[test]
    fn test_query_params_serialization() {
        let params = QueryParams {
            conversation_id: Some("conv_123".to_string()),
            since_msg_id: Some(100),
            limit: Some(50),
            order: Some("asc".to_string()),
        };
        
        let json = serde_json::to_string(&params).unwrap();
        let deserialized: QueryParams = serde_json::from_str(&json).unwrap();
        
        assert_eq!(params.conversation_id, deserialized.conversation_id);
        assert_eq!(params.since_msg_id, deserialized.since_msg_id);
        assert_eq!(params.limit, deserialized.limit);
        assert_eq!(params.order, deserialized.order);
    }
}
