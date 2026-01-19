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
    sql_url: String,
    http_client: reqwest::Client,
    auth: AuthProvider,
    max_retries: u32,
}

impl QueryExecutor {
    pub(crate) fn new(
        base_url: String,
        http_client: reqwest::Client,
        auth: AuthProvider,
        max_retries: u32,
    ) -> Self {
        Self {
            sql_url: format!("{}/v1/api/sql", base_url),
            http_client,
            auth,
            max_retries,
        }
    }

    fn is_retry_safe_sql(sql: &str) -> bool {
        // Very conservative: only retry simple, read-only statements.
        // Avoid retrying DDL/DML (CREATE/INSERT/UPDATE/DELETE/...) because
        // a request might succeed server-side but time out client-side.
        matches!(
            Self::first_keyword(sql).as_deref(),
            Some("SELECT" | "SHOW" | "DESCRIBE" | "EXPLAIN")
        )
    }

    fn first_keyword(sql: &str) -> Option<String> {
        let bytes = sql.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // Skip whitespace
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }

            // Skip line comments: -- ...\n
            if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }

            // Skip block comments: /* ... */
            if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < bytes.len() {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // Read keyword
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            if start == i {
                return None;
            }
            return Some(sql[start..i].to_ascii_uppercase());
        }

        None
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
        let mut retries: u32 = 0;
        let max_retries = self.max_retries;
        let retry_safe_sql = Self::is_retry_safe_sql(sql);

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
            let mut req_builder = self.http_client.post(&self.sql_url).json(&request);

            // Apply authentication
            req_builder = self.auth.apply_to_request(req_builder)?;

            let attempt_start = Instant::now();
            debug!(
                "[LINK_HTTP] Sending POST to {} (attempt {}/{})",
                self.sql_url,
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
                        let error_text =
                            response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

                        // SECURITY: Authentication/Authorization errors (4xx) must return an error,
                        // not Ok with error status. This prevents CLI from exiting with success code
                        // when auth fails, which could mask security issues in scripts/automation.
                        if status.is_client_error() {
                            // 401 Unauthorized, 403 Forbidden - always return error
                            let status_code = status.as_u16();
                            warn!(
                                "[LINK_HTTP] Authentication/client error: status={} message=\"{}\" duration_ms={}",
                                status, error_text, http_duration_ms
                            );
                            return Err(KalamLinkError::ServerError {
                                status_code,
                                message: error_text,
                            });
                        }

                        // For 5xx errors, prefer returning a structured QueryResponse if available,
                        // so SQL execution errors can be distinguished from transport errors.
                        if let Ok(mut json_response) =
                            serde_json::from_str::<QueryResponse>(&error_text)
                        {
                            normalize_query_response(sql, &mut json_response);
                            return Ok(json_response);
                        }

                        let error_message = error_text;

                        warn!(
                            "[LINK_HTTP] Server error: status={} message=\"{}\" duration_ms={}",
                            status, error_message, http_duration_ms
                        );

                        return Err(KalamLinkError::ServerError {
                            status_code: status.as_u16(),
                            message: error_message,
                        });
                    }
                },
                Err(e) if retry_safe_sql && retries < max_retries && Self::is_retriable(&e) => {
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
                },
                Err(e) => {
                    let http_duration_ms = attempt_start.elapsed().as_millis();
                    warn!(
                        "[LINK_HTTP] Fatal error: {} duration_ms={} total_ms={}",
                        e,
                        http_duration_ms,
                        overall_start.elapsed().as_millis()
                    );
                    return Err(e.into());
                },
            }
        }
    }

    fn is_retriable(err: &reqwest::Error) -> bool {
        err.is_timeout() || err.is_connect()
    }
}
