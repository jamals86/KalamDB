use std::time::Duration;

use kalam_link::{
    AuthProvider, KalamLinkClient, KalamLinkTimeouts, QueryResponse, ServerSetupRequest,
    SubscriptionConfig, SubscriptionManager,
};
use serde::Deserialize;
use serde_json::Value;

/// Thin wrapper around `KalamLinkClient` that provides convenient helpers
/// for the benchmark suite while delegating all HTTP / WebSocket work to kalam-link.
#[derive(Clone)]
pub struct KalamClient {
    link: KalamLinkClient,
    base_url: String,
}

// Keep these simple response types so existing benchmarks compile unchanged.
#[derive(Debug, Deserialize)]
pub struct SqlResponse {
    pub status: String,
    #[serde(default)]
    pub results: Vec<SqlResult>,
    pub took: Option<f64>,
    pub error: Option<SqlError>,
}

#[derive(Debug, Deserialize)]
pub struct SqlResult {
    pub row_count: Option<u64>,
    pub message: Option<String>,
    pub rows: Option<Vec<Vec<Value>>>,
}

#[derive(Debug, Deserialize)]
pub struct SqlError {
    pub code: Option<String>,
    pub message: String,
}

impl KalamClient {
    /// Create a new client by logging in via kalam-link and obtaining a JWT.
    /// Handles initial server setup if needed.
    pub async fn login(base_url: &str, username: &str, password: &str) -> Result<Self, String> {
        let base_url = base_url.trim_end_matches('/').to_string();

        // Build an unauthenticated client first — needed for setup + login
        let unauthed = KalamLinkClient::builder()
            .base_url(&base_url)
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| format!("failed to build kalam-link client: {}", e))?;

        // Complete setup if needed
        Self::complete_setup_if_needed(&unauthed, username, password).await;

        // Login
        let login_resp = match unauthed.login(username, password).await {
            Ok(r) => r,
            Err(kalam_link::KalamLinkError::SetupRequired(_)) => {
                // Try setup again + retry login
                Self::complete_setup_if_needed(&unauthed, username, password).await;
                unauthed
                    .login(username, password)
                    .await
                    .map_err(|e| format!("Login failed after setup: {}", e))?
            }
            Err(e) => return Err(format!("Login failed: {}", e)),
        };

        let token = login_resp.access_token.clone();

        // Build the authenticated client with sensible timeouts
        let timeouts = KalamLinkTimeouts::builder()
            .connection_timeout(Duration::from_secs(45))
            .receive_timeout(Duration::from_secs(60))
            .send_timeout(Duration::from_secs(30))
            .subscribe_timeout(Duration::from_secs(45))
            .auth_timeout(Duration::from_secs(30))
            .initial_data_timeout(Duration::from_secs(60))
            .build();

        let link = KalamLinkClient::builder()
            .base_url(&base_url)
            .auth(AuthProvider::jwt_token(token))
            .timeout(Duration::from_secs(60))
            .timeouts(timeouts)
            .build()
            .map_err(|e| format!("failed to build authenticated kalam-link client: {}", e))?;

        Ok(Self {
            link,
            base_url: base_url.to_string(),
        })
    }

    /// Complete initial server setup if the server hasn't been set up yet.
    async fn complete_setup_if_needed(
        client: &KalamLinkClient,
        username: &str,
        password: &str,
    ) {
        let Ok(status) = client.check_setup_status().await else {
            return;
        };
        if !status.needs_setup {
            return;
        }
        eprintln!("  Server needs initial setup, running setup...");
        let req = ServerSetupRequest::new(
            username.to_string(),
            password.to_string(),
            password.to_string(),
            None,
        );
        let _ = client.server_setup(req).await;
    }

    /// Execute a SQL statement returning a `QueryResponse` from kalam-link.
    pub async fn query(&self, sql: &str) -> Result<QueryResponse, String> {
        self.link
            .execute_query(sql, None, None, None)
            .await
            .map_err(|e| format!("SQL error: {}", e))
    }

    /// Execute a SQL statement and return the parsed response (legacy format).
    pub async fn sql(&self, sql: &str) -> Result<SqlResponse, String> {
        let resp = self
            .link
            .execute_query(sql, None, None, None)
            .await
            .map_err(|e| format!("SQL error: {}", e))?;
        Ok(query_response_to_sql_response(resp))
    }

    /// Execute SQL expecting success, return error message on failure.
    pub async fn sql_ok(&self, sql: &str) -> Result<SqlResponse, String> {
        let resp = self.sql(sql).await?;
        if resp.status != "success" {
            let msg = resp
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "unknown error".to_string());
            return Err(format!("SQL error: {}", msg));
        }
        Ok(resp)
    }

    /// Health check — can we reach the server?
    pub async fn health_check(&self) -> bool {
        self.link.health_check().await.is_ok()
    }

    /// Subscribe to a live query using kalam-link's built-in subscription manager.
    pub async fn subscribe(&self, sql: &str) -> Result<SubscriptionManager, String> {
        self.link
            .subscribe(sql)
            .await
            .map_err(|e| format!("Subscribe error: {}", e))
    }

    /// Subscribe with a full config (custom subscription ID, options, etc.).
    pub async fn subscribe_with_config(
        &self,
        config: SubscriptionConfig,
    ) -> Result<SubscriptionManager, String> {
        self.link
            .subscribe_with_config(config)
            .await
            .map_err(|e| format!("Subscribe error: {}", e))
    }

    /// Get a reference to the inner kalam-link client (for advanced use).
    pub fn link(&self) -> &KalamLinkClient {
        &self.link
    }

    /// Returns the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Convert a kalam-link `QueryResponse` into the legacy `SqlResponse` used by benchmarks.
fn query_response_to_sql_response(resp: QueryResponse) -> SqlResponse {
    let status = if resp.success() {
        "success".to_string()
    } else {
        "error".to_string()
    };

    let error = resp.error.as_ref().map(|e| SqlError {
        code: Some(e.code.clone()),
        message: e.message.clone(),
    });

    let results = resp
        .results
        .iter()
        .map(|r| SqlResult {
            row_count: Some(r.row_count as u64),
            message: r.message.clone(),
            rows: r.rows.clone(),
        })
        .collect();

    SqlResponse {
        status,
        results,
        took: resp.took,
        error,
    }
}
