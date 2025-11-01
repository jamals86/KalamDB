//! SQL query execution with HTTP transport.

use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    models::{QueryRequest, QueryResponse},
};
use crate::normalize::normalize_query_response;

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

    /// Execute a SQL query with optional parameters
    pub async fn execute(
        &self,
        sql: &str,
        params: Option<Vec<serde_json::Value>>,
    ) -> Result<QueryResponse> {
        let request = QueryRequest {
            sql: sql.to_string(),
            params,
        };

        // Send request with retry logic
        let mut retries = 0;
        let max_retries = 3;

        loop {
            // Build request fresh on each attempt (can't clone request builders with bodies)
            let url = format!("{}/v1/api/sql", self.base_url);
            let mut req_builder = self.http_client.post(&url).json(&request);

            // Apply authentication
            req_builder = self.auth.apply_to_request(req_builder)?;

            match req_builder.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let mut query_response: QueryResponse = response.json().await?;
                        // Enforce stable column ordering where applicable
                        normalize_query_response(sql, &mut query_response);
                        return Ok(query_response);
                    } else {
                        let status = response.status();
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

                        return Err(KalamLinkError::ServerError {
                            status_code: status.as_u16(),
                            message: error_message,
                        });
                    }
                }
                Err(e) if retries < max_retries && Self::is_retriable(&e) => {
                    retries += 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(100 * retries as u64))
                        .await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn is_retriable(err: &reqwest::Error) -> bool {
        err.is_timeout() || err.is_connect()
    }
}
