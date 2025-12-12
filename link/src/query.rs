//! SQL query execution with HTTP transport.

use crate::normalize::normalize_query_response;
use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    models::{QueryRequest, QueryResponse},
};
use log::{debug, warn};
use std::time::Instant;

/// Handles SQL query execution via HTTP.
#[derive(Clone)]
pub struct QueryExecutor {
    base_url: String,
    http_client: reqwest::Client,
    auth: AuthProvider,
}

impl QueryExecutor {
    pub(crate) fn new(base_url: String, http_client: reqwest::Client, auth: AuthProvider) -> Self {
        Self {
            base_url,
            http_client,
            auth,
        }
    }

    /// Execute a SQL query with optional parameters and namespace
    pub async fn execute(
        &self,
        sql: &str,
        params: Option<Vec<serde_json::Value>>,
        namespace_id: Option<String>,
    ) -> Result<QueryResponse> {
        let request = QueryRequest {
            sql: sql.to_string(),
            params,
            namespace_id,
        };

        // Send request with retry logic
        let mut retries = 0;
        let max_retries = 3;
        
        // Log query start
        let sql_preview = if sql.len() > 80 {
            format!("{}...", &sql[..80])
        } else {
            sql.to_string()
        };
        debug!(
            "[LINK_QUERY] Starting query: \"{}\" (len={})",
            sql_preview.replace('\n', " "),
            sql.len()
        );

        let overall_start = Instant::now();

        loop {
            // Build request fresh on each attempt (can't clone request builders with bodies)
            let url = format!("{}/v1/api/sql", self.base_url);
            let mut req_builder = self.http_client.post(&url).json(&request);

            // Apply authentication
            req_builder = self.auth.apply_to_request(req_builder)?;

            let attempt_start = Instant::now();
            debug!(
                "[LINK_HTTP] Sending POST to {} (attempt {}/{})",
                url,
                retries + 1,
                max_retries + 1
            );

            match req_builder.send().await {
                Ok(response) => {
                    let http_duration_ms = attempt_start.elapsed().as_millis();
                    let status = response.status();
                    debug!(
                        "[LINK_HTTP] Response received: status={} duration_ms={}",
                        status, http_duration_ms
                    );

                    if status.is_success() {
                        let parse_start = Instant::now();
                        let mut query_response: QueryResponse = response.json().await?;
                        let parse_duration_ms = parse_start.elapsed().as_millis();
                        
                        // Enforce stable column ordering where applicable
                        normalize_query_response(sql, &mut query_response);
                        
                        let total_duration_ms = overall_start.elapsed().as_millis();
                        debug!(
                            "[LINK_QUERY] Success: http_ms={} parse_ms={} total_ms={}",
                            http_duration_ms, parse_duration_ms, total_duration_ms
                        );
                        
                        return Ok(query_response);
                    } else {
                        let error_text = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());

                        // Try to parse as QueryResponse to extract error message
                        let error_message = if let Ok(json_response) =
                            serde_json::from_str::<QueryResponse>(&error_text)
                        {
                            if let Some(err) = json_response.error {
                                err.message
                            } else {
                                error_text
                            }
                        } else {
                            error_text
                        };

                        warn!(
                            "[LINK_HTTP] Server error: status={} message=\"{}\" duration_ms={}",
                            status, error_message, http_duration_ms
                        );

                        return Err(KalamLinkError::ServerError {
                            status_code: status.as_u16(),
                            message: error_message,
                        });
                    }
                }
                Err(e) if retries < max_retries && Self::is_retriable(&e) => {
                    let http_duration_ms = attempt_start.elapsed().as_millis();
                    warn!(
                        "[LINK_HTTP] Retriable error (attempt {}/{}): {} duration_ms={}",
                        retries + 1,
                        max_retries + 1,
                        e,
                        http_duration_ms
                    );
                    retries += 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(100 * retries as u64))
                        .await;
                    continue;
                }
                Err(e) => {
                    let http_duration_ms = attempt_start.elapsed().as_millis();
                    warn!(
                        "[LINK_HTTP] Fatal error: {} duration_ms={} total_ms={}",
                        e,
                        http_duration_ms,
                        overall_start.elapsed().as_millis()
                    );
                    return Err(e.into());
                }
            }
        }
    }

    fn is_retriable(err: &reqwest::Error) -> bool {
        err.is_timeout() || err.is_connect()
    }
}
