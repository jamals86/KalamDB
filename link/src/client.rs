//! Main KalamDB client with builder pattern.
//!
//! Provides the primary interface for connecting to KalamDB servers
//! and executing operations.

use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    models::{ConnectionOptions, HealthCheckResponse, HttpVersion, QueryResponse, SubscriptionConfig},
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
/// let response = client.execute_query("SELECT 1", None, None).await?;
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

    /// Execute a SQL query with optional parameters and namespace context
    ///
    /// # Arguments
    /// * `sql` - The SQL query string
    /// * `params` - Optional query parameters for $1, $2, ... placeholders
    /// * `namespace_id` - Optional namespace for unqualified table names
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = kalam_link::KalamLinkClient::builder().base_url("http://localhost:3000").build()?;
    /// // Simple query
    /// let result = client.execute_query("SELECT * FROM users", None, None).await?;
    ///
    /// // Query with parameters
    /// let params = vec![serde_json::json!(42)];
    /// let result = client.execute_query("SELECT * FROM users WHERE id = $1", Some(params), None).await?;
    ///
    /// // Query in specific namespace
    /// let result = client.execute_query("SELECT * FROM messages", None, Some("chat")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_query(
        &self,
        sql: &str,
        params: Option<Vec<serde_json::Value>>,
        namespace_id: Option<&str>,
    ) -> Result<QueryResponse> {
        self.query_executor
            .execute(sql, params, namespace_id.map(|s| s.to_string()))
            .await
    }

    /// Subscribe to real-time changes
    pub async fn subscribe(&self, query: &str) -> Result<SubscriptionManager> {
        // Generate a unique subscription ID using timestamp + random component
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let subscription_id = format!("sub_{}", nanos);
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

    /// Login with username and password to obtain a JWT token
    ///
    /// This method authenticates with the server and returns a JWT access token
    /// that can be used for subsequent API calls via `AuthProvider::jwt_token()`.
    ///
    /// # Arguments
    /// * `username` - The username for authentication
    /// * `password` - The password for authentication
    ///
    /// # Returns
    /// A `LoginResponse` containing the JWT access token and user information
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use kalam_link::{KalamLinkClient, AuthProvider};
    ///
    /// // Create a client without authentication to perform login
    /// let client = KalamLinkClient::builder()
    ///     .base_url("http://localhost:3000")
    ///     .build()?;
    ///
    /// // Login to get JWT token
    /// let login_response = client.login("alice", "secret123").await?;
    ///
    /// // Create a new client with the JWT token for subsequent calls
    /// let authenticated_client = KalamLinkClient::builder()
    ///     .base_url("http://localhost:3000")
    ///     .auth(AuthProvider::jwt_token(login_response.access_token))
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn login(&self, username: &str, password: &str) -> Result<crate::models::LoginResponse> {
        let url = format!("{}/v1/api/auth/login", self.base_url);
        log::debug!("[LOGIN] Authenticating user '{}' at url={}", username, url);
        
        let login_request = crate::models::LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };
        
        let start = std::time::Instant::now();
        let response = self.http_client
            .post(&url)
            .json(&login_request)
            .send()
            .await?;
        
        let status = response.status();
        log::debug!("[LOGIN] HTTP response received in {:?}, status={}", start.elapsed(), status);
        
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            log::debug!("[LOGIN] Login failed: {}", error_text);
            return Err(KalamLinkError::AuthenticationError(
                format!("Login failed ({}): {}", status, error_text)
            ));
        }
        
        let login_response = response.json::<crate::models::LoginResponse>().await?;
        log::debug!("[LOGIN] Successfully authenticated user '{}' in {:?}", username, start.elapsed());
        
        Ok(login_response)
    }
}

/// Builder for configuring [`KalamLinkClient`] instances.
pub struct KalamLinkClientBuilder {
    base_url: Option<String>,
    timeout: Duration,
    auth: AuthProvider,
    max_retries: u32,
    timeouts: KalamLinkTimeouts,
    connection_options: ConnectionOptions,
}

impl KalamLinkClientBuilder {
    fn new() -> Self {
        Self {
            base_url: None,
            timeout: Duration::from_secs(30),
            auth: AuthProvider::none(),
            max_retries: 3,
            timeouts: KalamLinkTimeouts::default(),
            connection_options: ConnectionOptions::default(),
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

    /// Set connection options for HTTP and WebSocket behavior
    ///
    /// This allows configuring HTTP version, reconnection behavior, and other
    /// connection-level settings.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use kalam_link::{KalamLinkClient, ConnectionOptions, HttpVersion};
    ///
    /// # async fn example() -> kalam_link::Result<()> {
    /// let client = KalamLinkClient::builder()
    ///     .base_url("http://localhost:3000")
    ///     .connection_options(
    ///         ConnectionOptions::new()
    ///             .with_http_version(HttpVersion::Http2)
    ///             .with_auto_reconnect(true)
    ///     )
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn connection_options(mut self, options: ConnectionOptions) -> Self {
        self.connection_options = options;
        self
    }

    /// Set the HTTP protocol version to use
    ///
    /// Shorthand for setting just the HTTP version without other connection options.
    ///
    /// - `HttpVersion::Http1` - HTTP/1.1 (default, maximum compatibility)
    /// - `HttpVersion::Http2` - HTTP/2 (multiplexing, better for concurrent requests)
    /// - `HttpVersion::Auto` - Let the client negotiate with the server
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use kalam_link::{KalamLinkClient, HttpVersion};
    ///
    /// # async fn example() -> kalam_link::Result<()> {
    /// let client = KalamLinkClient::builder()
    ///     .base_url("http://localhost:3000")
    ///     .http_version(HttpVersion::Http2)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn http_version(mut self, version: HttpVersion) -> Self {
        self.connection_options.http_version = version;
        self
    }

    /// Build the client
    pub fn build(self) -> Result<KalamLinkClient> {
        let base_url = self
            .base_url
            .ok_or_else(|| KalamLinkError::ConfigurationError("base_url is required".into()))?;

        // Build HTTP client with connection pooling for better throughput
        // Keep-alive connections reduce TCP handshake overhead significantly
        let mut client_builder = reqwest::Client::builder()
            .timeout(self.timeout)
            .connect_timeout(self.timeouts.connection_timeout)
            // Enable connection pooling for high throughput (keep-alive)
            .pool_max_idle_per_host(10)
            // Keep idle connections for 90 seconds (slightly longer than server's 75s)
            .pool_idle_timeout(std::time::Duration::from_secs(90));

        // Configure HTTP version based on connection options
        client_builder = match self.connection_options.http_version {
            HttpVersion::Http1 => {
                log::debug!("[CLIENT] Using HTTP/1.1 only");
                client_builder.http1_only()
            }
            HttpVersion::Http2 => {
                log::debug!("[CLIENT] Using HTTP/2 with prior knowledge");
                // http2_prior_knowledge() assumes the server speaks HTTP/2
                // Use this for known HTTP/2 servers for best performance
                client_builder.http2_prior_knowledge()
            }
            HttpVersion::Auto => {
                log::debug!("[CLIENT] Using automatic HTTP version negotiation");
                // Default behavior - will negotiate via ALPN for HTTPS
                // For HTTP, will use HTTP/1.1
                client_builder
            }
        };

        let http_client = client_builder
            .build()
            .map_err(|e| KalamLinkError::ConfigurationError(e.to_string()))?;

        let query_executor = QueryExecutor::new(
            base_url.clone(),
            http_client.clone(),
            self.auth.clone(),
            self.max_retries,
        );

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
