//! Main KalamDB client with builder pattern.
//!
//! Provides the primary interface for connecting to KalamDB servers
//! and executing operations.

use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    models::QueryResponse,
    query::QueryExecutor,
    subscription::SubscriptionManager,
};
use std::time::Duration;

/// Main KalamDB client.
///
/// Use [`KalamLinkClientBuilder`] to construct instances with custom configuration.
///
/// # Examples
///
/// ```rust,no_run
/// use kalam_link::KalamLinkClient;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let client = KalamLinkClient::builder()
///     .base_url("http://localhost:3000")
///     .timeout(std::time::Duration::from_secs(30))
///     .build()?;
///
/// let response = client.execute_query("SELECT 1").await?;
/// println!("Result: {:?}", response);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct KalamLinkClient {
    base_url: String,
    http_client: reqwest::Client,
    auth: AuthProvider,
    query_executor: QueryExecutor,
}

impl KalamLinkClient {
    /// Create a new builder for configuring the client
    pub fn builder() -> KalamLinkClientBuilder {
        KalamLinkClientBuilder::new()
    }

    /// Execute a SQL query
    pub async fn execute_query(&self, sql: &str) -> Result<QueryResponse> {
        self.query_executor.execute(sql, None).await
    }

    /// Execute a SQL query with parameters
    pub async fn execute_query_with_params(
        &self,
        sql: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<QueryResponse> {
        self.query_executor.execute(sql, Some(params)).await
    }

    /// Subscribe to real-time changes
    pub async fn subscribe(&self, query: &str) -> Result<SubscriptionManager> {
        SubscriptionManager::new(&self.base_url, query, &self.auth).await
    }

    /// Check server health
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        let response = self.http_client.get(&url).send().await?;
        Ok(response.status().is_success())
    }
}

/// Builder for configuring [`KalamLinkClient`] instances.
pub struct KalamLinkClientBuilder {
    base_url: Option<String>,
    timeout: Duration,
    auth: AuthProvider,
    max_retries: u32,
    user_id: Option<String>,
}

impl KalamLinkClientBuilder {
    fn new() -> Self {
        Self {
            base_url: None,
            timeout: Duration::from_secs(30),
            auth: AuthProvider::none(),
            max_retries: 3,
            user_id: None,
        }
    }

    /// Set the base URL for the KalamDB server
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set JWT token authentication
    pub fn jwt_token(mut self, token: impl Into<String>) -> Self {
        self.auth = AuthProvider::jwt_token(token.into());
        self
    }

    /// Set API key authentication
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.auth = AuthProvider::api_key(key.into());
        self
    }

    /// Set maximum number of retries for failed requests
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set user ID for X-USER-ID header
    pub fn user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Build the client
    pub fn build(self) -> Result<KalamLinkClient> {
        let base_url = self
            .base_url
            .ok_or_else(|| KalamLinkError::ConfigurationError("base_url is required".into()))?;

        let http_client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| KalamLinkError::ConfigurationError(e.to_string()))?;

        let query_executor =
            QueryExecutor::new(base_url.clone(), http_client.clone(), self.auth.clone(), self.user_id.clone());

        Ok(KalamLinkClient {
            base_url,
            http_client,
            auth: self.auth,
            query_executor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_pattern() {
        let result = KalamLinkClient::builder()
            .base_url("http://localhost:3000")
            .timeout(Duration::from_secs(10))
            .jwt_token("test_token")
            .build();

        assert!(result.is_ok());
    }

    #[test]
    fn test_builder_missing_url() {
        let result = KalamLinkClient::builder().build();
        assert!(result.is_err());
    }
}
