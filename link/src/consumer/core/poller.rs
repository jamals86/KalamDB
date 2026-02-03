use crate::auth::AuthProvider;
use crate::consumer::models::CommitResult;
use crate::consumer::models::consumer_record::ConsumerRecordWire;
use crate::error::{KalamLinkError, Result};
use crate::consumer::models::AutoOffsetReset;
use crate::consumer::utils::backoff::jittered_exponential_backoff;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct ConsumerPoller {
    consume_url: String,
    ack_url: String,
    http_client: reqwest::Client,
    auth: AuthProvider,
    request_timeout: Duration,
    retry_backoff: Duration,
    max_retries: u32,
}

impl ConsumerPoller {
    pub fn new(
        base_url: &str,
        http_client: reqwest::Client,
        auth: AuthProvider,
        request_timeout: Duration,
        retry_backoff: Duration,
    ) -> Self {
        Self {
            consume_url: format!("{}/api/topics/consume", base_url.trim_end_matches('/')),
            ack_url: format!("{}/api/topics/ack", base_url.trim_end_matches('/')),
            http_client,
            auth,
            request_timeout,
            retry_backoff,
            max_retries: 3,
        }
    }

    pub async fn consume(&self, request: ConsumeRequest) -> Result<ConsumeResponse> {
        let mut attempt: u32 = 0;
        let max_retries = self.max_retries;

        loop {
            let mut req_builder = self.http_client.post(&self.consume_url).json(&request);
            req_builder = req_builder.timeout(self.request_timeout);
            req_builder = self.auth.apply_to_request(req_builder)?;

            let attempt_start = std::time::Instant::now();
            debug!(
                "[LINK_CONSUMER] consume request attempt {}/{}",
                attempt + 1,
                max_retries + 1
            );

            match req_builder.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let result = response.json::<ConsumeResponse>().await?;
                        return Ok(result);
                    }

                    let error_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());

                    if status.is_client_error() {
                        return Err(KalamLinkError::ServerError {
                            status_code: status.as_u16(),
                            message: error_text,
                        });
                    }

                    if attempt < max_retries && is_retriable_status(status.as_u16()) {
                        let delay = jittered_exponential_backoff(
                            self.retry_backoff,
                            attempt,
                            Duration::from_secs(10),
                        );
                        warn!(
                            "[LINK_CONSUMER] Retriable consume error: status={} delay_ms={} duration_ms={}",
                            status,
                            delay.as_millis(),
                            attempt_start.elapsed().as_millis()
                        );
                        attempt += 1;
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    return Err(KalamLinkError::ServerError {
                        status_code: status.as_u16(),
                        message: error_text,
                    });
                },
                Err(err) if is_retriable_error(&err) && attempt < max_retries => {
                    let delay = jittered_exponential_backoff(
                        self.retry_backoff,
                        attempt,
                        Duration::from_secs(10),
                    );
                    warn!(
                        "[LINK_CONSUMER] Retriable consume error: {} delay_ms={} duration_ms={}",
                        err,
                        delay.as_millis(),
                        attempt_start.elapsed().as_millis()
                    );
                    attempt += 1;
                    tokio::time::sleep(delay).await;
                },
                Err(err) => return Err(err.into()),
            }
        }
    }

    pub async fn ack(&self, request: AckRequest) -> Result<CommitResult> {
        let mut req_builder = self.http_client.post(&self.ack_url).json(&request);
        req_builder = req_builder.timeout(self.request_timeout);
        req_builder = self.auth.apply_to_request(req_builder)?;

        let response = req_builder.send().await?;
        let status = response.status();

        if status.is_success() {
            let ack_response = response.json::<AckResponse>().await?;
            return Ok(CommitResult {
                acknowledged_offset: ack_response.acknowledged_offset,
                group_id: request.group_id,
                partition_id: request.partition_id,
            });
        }

        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        Err(KalamLinkError::ServerError {
            status_code: status.as_u16(),
            message: error_text,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsumeRequest {
    pub topic: String,
    pub group_id: String,
    pub start: AutoOffsetReset,
    pub limit: u32,
    pub partition_id: u32,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConsumeResponse {
    pub messages: Vec<ConsumerRecordWire>,
    pub next_offset: u64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AckRequest {
    pub topic: String,
    pub group_id: String,
    pub partition_id: u32,
    pub upto_offset: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct AckResponse {
    pub success: bool,
    pub acknowledged_offset: u64,
}

fn is_retriable_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect()
}

fn is_retriable_status(status_code: u16) -> bool {
    matches!(status_code, 500 | 502 | 503 | 504)
}
