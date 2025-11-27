//! Main KalamDB client with builder pattern.
//!
//! Provides the primary interface for connecting to KalamDB servers
//! and executing operations.

use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    models::{HealthCheckResponse, QueryResponse, SubscriptionConfig},
    query::QueryExecutor,
    subscription::SubscriptionManager,
    timeouts::KalamLinkTimeouts,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

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
    health_cache: Arc<Mutex<HealthCheckCache>>,
    timeouts: KalamLinkTimeouts,
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
        // Generate a unique subscription ID using timestamp + random component
        let subscription_id = format!("sub_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos());
        self.subscribe_with_config(SubscriptionConfig::new(subscription_id, query))
            .await
    }

    /// Subscribe with advanced configuration (pre-generated ID, options, ws_url override)
    pub async fn subscribe_with_config(
        &self,
        config: SubscriptionConfig,
    ) -> Result<SubscriptionManager> {
        SubscriptionManager::new(&self.base_url, config, &self.auth, &self.timeouts).await
    }

    /// Get the configured timeouts
    pub fn timeouts(&self) -> &KalamLinkTimeouts {
        &self.timeouts
    }

    /// Check server health and get server information
    pub async fn health_check(&self) -> Result<HealthCheckResponse> {
        {
            let cache = self.health_cache.lock().await;
            if let (Some(last_check), Some(response)) =
                (cache.last_check, cache.last_response.clone())
            {
                if last_check.elapsed() < HEALTH_CHECK_TTL {
                    log::debug!("[HEALTH_CHECK] Returning cached response (age: {:?})", last_check.elapsed());
                    return Ok(response);
                }
            }
        }

        let url = format!("{}/v1/api/healthcheck", self.base_url);
        log::debug!("[HEALTH_CHECK] Fetching from url={}", url);
        let start = std::time::Instant::now();
        let response = self.http_client.get(&url).send().await?;
        log::debug!("[HEALTH_CHECK] HTTP response received in {:?}, status={}", start.elapsed(), response.status());
        let health_response = response.json::<HealthCheckResponse>().await?;
        log::debug!("[HEALTH_CHECK] JSON parsed in {:?}", start.elapsed());

        let mut cache = self.health_cache.lock().await;
        cache.last_check = Some(Instant::now());
        cache.last_response = Some(health_response.clone());

        Ok(health_response)
    }
}

/// Builder for configuring [`KalamLinkClient`] instances.
pub struct KalamLinkClientBuilder {
    base_url: Option<String>,
    timeout: Duration,
    auth: AuthProvider,
    max_retries: u32,
    timeouts: KalamLinkTimeouts,
}

impl KalamLinkClientBuilder {
    fn new() -> Self {
        Self {
            base_url: None,
            timeout: Duration::from_secs(30),
            auth: AuthProvider::none(),
            max_retries: 3,
            timeouts: KalamLinkTimeouts::default(),
        }
    }

    /// Set the base URL for the KalamDB server
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set request timeout (for HTTP requests)
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set JWT token authentication
    pub fn jwt_token(mut self, token: impl Into<String>) -> Self {
        self.auth = AuthProvider::jwt_token(token.into());
        self
    }

    /// Set authentication provider directly
    ///
    /// Allows setting any AuthProvider variant including BasicAuth.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use kalam_link::{KalamLinkClient, AuthProvider};
    ///
    /// # async fn example() -> kalam_link::Result<()> {
    /// let client = KalamLinkClient::builder()
    ///     .base_url("http://localhost:3000")
    ///     .auth(AuthProvider::basic_auth("alice".to_string(), "secret".to_string()))
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn auth(mut self, auth: AuthProvider) -> Self {
        self.auth = auth;
        self
    }

    /// Set maximum number of retries for failed requests
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set comprehensive timeout configuration for all operations
    ///
    /// This overrides individual timeout settings like `timeout()`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use kalam_link::{KalamLinkClient, KalamLinkTimeouts};
    ///
    /// # async fn example() -> kalam_link::Result<()> {
    /// let client = KalamLinkClient::builder()
    ///     .base_url("http://localhost:3000")
    ///     .timeouts(KalamLinkTimeouts::fast())
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn timeouts(mut self, timeouts: KalamLinkTimeouts) -> Self {
        // Also update the HTTP timeout to match
        self.timeout = timeouts.receive_timeout;
        self.timeouts = timeouts;
        self
    }

    /// Build the client
    pub fn build(self) -> Result<KalamLinkClient> {
        use reqwest::header::{HeaderMap, HeaderValue, CONNECTION};
        
        let base_url = self
            .base_url
            .ok_or_else(|| KalamLinkError::ConfigurationError("base_url is required".into()))?;

        // Set Connection: close header to force immediate connection closure
        // This prevents CLOSE_WAIT accumulation and 30s delays
        let mut default_headers = HeaderMap::new();
        default_headers.insert(CONNECTION, HeaderValue::from_static("close"));

        let http_client = reqwest::Client::builder()
            .timeout(self.timeout)
            .connect_timeout(self.timeouts.connection_timeout)
            // Disable connection pooling completely
            .pool_max_idle_per_host(0)
            // Set idle timeout to 0 to immediately close unused connections  
            .pool_idle_timeout(std::time::Duration::from_millis(0))
            // Add Connection: close header to all requests
            .default_headers(default_headers)
            .build()
            .map_err(|e| KalamLinkError::ConfigurationError(e.to_string()))?;

        let query_executor =
            QueryExecutor::new(base_url.clone(), http_client.clone(), self.auth.clone());

        Ok(KalamLinkClient {
            base_url,
            http_client,
            auth: self.auth,
            query_executor,
            health_cache: Arc::new(Mutex::new(HealthCheckCache::default())),
            timeouts: self.timeouts,
        })
    }
}

const HEALTH_CHECK_TTL: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
struct HealthCheckCache {
    last_check: Option<Instant>,
    last_response: Option<HealthCheckResponse>,
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
