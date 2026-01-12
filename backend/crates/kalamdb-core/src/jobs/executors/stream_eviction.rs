//! Stream Eviction Job Executor
//!
//! **Phase 9 (T149)**: JobExecutor implementation for stream table eviction
//!
//! Handles TTL-based eviction for stream tables.
//!
//! ## Responsibilities
//! - Enforce ttl_seconds policy for stream tables
//! - Delete expired records based on created_at timestamp
//! - Track eviction metrics (rows evicted, bytes freed)
//! - Support partial eviction with continuation
//!
//! ## Parameters Format
//! ```json
//! {
//!   "namespace_id": "default",
//!   "table_name": "events",
//!   "table_type": "Stream",
//!   "ttl_seconds": 86400,
//!   "batch_size": 10000
//! }
//! ```

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::jobs::executors::{JobContext, JobDecision, JobExecutor, JobParams};
use crate::providers::StreamTableProvider;
use async_trait::async_trait;
use kalamdb_commons::ids::{SeqId, SnowflakeGenerator, StreamTableRowId};
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::{JobType, TableId};
use kalamdb_store::entity_store::{EntityStore, ScanDirection};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn default_batch_size() -> u64 {
    10000
}

/// Typed parameters for stream eviction operations (T193)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvictionParams {
    /// Table identifier (required)
    pub table_id: TableId,
    /// Table type (must be Stream - validated in validate())
    pub table_type: TableType,
    /// TTL in seconds (required, must be > 0)
    pub ttl_seconds: u64,
    /// Batch size for eviction (optional, defaults to 10000)
    #[serde(default = "default_batch_size")]
    pub batch_size: u64,
}

impl JobParams for StreamEvictionParams {
    fn validate(&self) -> Result<(), KalamDbError> {
        if self.table_type != TableType::Stream {
            return Err(KalamDbError::InvalidOperation(format!(
                "table_type must be Stream, got: {:?}",
                self.table_type
            )));
        }
        if self.ttl_seconds == 0 {
            return Err(KalamDbError::InvalidOperation(
                "ttl_seconds must be greater than 0".to_string(),
            ));
        }
        if self.batch_size == 0 {
            return Err(KalamDbError::InvalidOperation(
                "batch_size must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Stream Eviction Job Executor
///
/// Executes TTL-based eviction operations for stream tables.
pub struct StreamEvictionExecutor;

impl StreamEvictionExecutor {
    /// Create a new StreamEvictionExecutor
    pub fn new() -> Self {
        Self
    }
}

fn compute_cutoff_window(ttl_seconds: u64) -> Result<(u64, SeqId), KalamDbError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .into_invalid_operation("System time error")?
        .as_millis() as u64;
    let ttl_ms = ttl_seconds.saturating_mul(1000);
    let cutoff_ms = now.saturating_sub(ttl_ms);
    let normalized_cutoff = cutoff_ms.max(SnowflakeGenerator::DEFAULT_EPOCH);
    let cutoff_seq = SeqId::max_id_for_timestamp(normalized_cutoff).map_err(|e| {
        KalamDbError::InvalidOperation(format!("Failed to compute cutoff snowflake id: {}", e))
    })?;
    Ok((cutoff_ms, cutoff_seq))
}

#[async_trait]
impl JobExecutor for StreamEvictionExecutor {
    type Params = StreamEvictionParams;

    fn job_type(&self) -> JobType {
        JobType::StreamEviction
    }

    fn name(&self) -> &'static str {
        "StreamEvictionExecutor"
    }

    async fn pre_validate(
        &self,
        app_ctx: &Arc<AppContext>,
        params: &Self::Params,
    ) -> Result<bool, KalamDbError> {
        params.validate()?;

        let schema_registry = app_ctx.schema_registry();
        let provider_arc = match schema_registry.get_provider(&params.table_id) {
            Some(p) => p,
            None => return Ok(false),
        };
        let stream_provider = provider_arc
            .as_any()
            .downcast_ref::<StreamTableProvider>()
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "Cached provider for {} is not a StreamTableProvider",
                    params.table_id
                ))
            })?;
        let store = stream_provider.store_arc();
        let (cutoff_ms, cutoff_seq) = compute_cutoff_window(params.ttl_seconds)?;

        const KEY_SCAN_LIMIT: usize = 512;
        let mut start_key: Option<StreamTableRowId> = None;

        loop {
            let iterator = store
                .scan_directional(start_key.as_ref(), ScanDirection::Newer, KEY_SCAN_LIMIT)
                .map_err(|e| {
                    KalamDbError::InvalidOperation(format!(
                        "Failed to scan stream keys during pre-validation: {}",
                        e
                    ))
                })?;

            let mut batch_count = 0usize;
            let mut last_key_in_batch: Option<StreamTableRowId> = None;

            for result in iterator {
                let (row_key, row) = result.map_err(|e| {
                    KalamDbError::InvalidOperation(format!(
                        "Failed to deserialize row during pre-validation: {}",
                        e
                    ))
                })?;
                batch_count += 1;

                if row._seq.timestamp_millis() < cutoff_ms {
                    return Ok(true);
                }

                if row_key.seq() <= cutoff_seq {
                    return Ok(true);
                }

                last_key_in_batch = Some(row_key);
            }

            if batch_count < KEY_SCAN_LIMIT {
                return Ok(false);
            }

            if let Some(last_key) = last_key_in_batch {
                start_key = Some(last_key);
            } else {
                return Ok(false);
            }
        }
    }

    async fn execute(&self, ctx: &JobContext<Self::Params>) -> Result<JobDecision, KalamDbError> {
        ctx.log_info("Starting stream eviction operation");

        // Parameters already validated in JobContext - type-safe access
        let params = ctx.params();
        let table_id = params.table_id.clone();
        let ttl_seconds = params.ttl_seconds;
        let batch_size = params.batch_size;

        ctx.log_info(&format!(
            "Evicting expired records from stream {} (ttl: {}s, batch: {})",
            table_id, ttl_seconds, batch_size
        ));

        // Calculate cutoff time for eviction (records created before this time are expired)
        let (cutoff_ms, cutoff_seq) = compute_cutoff_window(ttl_seconds)?;

        ctx.log_info(&format!(
            "Cutoff time: {}ms (records with timestamp < {} are expired)",
            cutoff_ms, cutoff_ms
        ));

        // Get table's StreamTableStore
        // Use the registered StreamTableProvider so we access the live in-memory store
        let schema_registry = ctx.app_ctx.schema_registry();
        let provider_arc = match schema_registry.get_provider(&table_id) {
            Some(p) => p,
            None => {
                ctx.log_warn(&format!(
                    "Stream provider not registered for {}; skipping eviction",
                    table_id
                ));
                return Ok(JobDecision::Completed {
                    message: Some(format!(
                        "Stream table {} not registered; nothing to evict",
                        table_id
                    )),
                });
            }
        };
        let stream_provider = provider_arc
            .as_any()
            .downcast_ref::<StreamTableProvider>()
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "Cached provider for {} is not a StreamTableProvider",
                    table_id
                ))
            })?;
        let store = stream_provider.store_arc();

        let batch_target = batch_size.min(usize::MAX as u64) as usize;
        const SCAN_CHUNK_MAX: usize = 1024;
        let scan_chunk = batch_target.clamp(1, SCAN_CHUNK_MAX);

        let mut expired_keys: Vec<StreamTableRowId> = Vec::new();
        let mut start_key: Option<StreamTableRowId> = None;
        
        // Track current user to optimize scanning (skip fresh rows for same user)
        let mut current_user_id: Option<kalamdb_commons::models::UserId> = None;
        let mut skip_current_user = false;

        while expired_keys.len() < batch_target {
            let iterator = store
                .scan_directional(start_key.as_ref(), ScanDirection::Newer, scan_chunk)
                .map_err(|e| {
                    KalamDbError::InvalidOperation(format!(
                        "Failed to scan stream rows for eviction: {}",
                        e
                    ))
                })?;

            let mut batch_count = 0usize;
            let mut last_processed_key: Option<StreamTableRowId> = None;

            for item in iterator {
                let (row_key, _) = item.into_invalid_operation("Failed to iterate stream rows")?;
                batch_count += 1;
                last_processed_key = Some(row_key.clone());

                // Check if we switched to a new user
                if Some(row_key.user_id()) != current_user_id.as_ref() {
                    current_user_id = Some(row_key.user_id().clone());
                    skip_current_user = false;
                }

                if skip_current_user {
                    continue;
                }

                // Check expiration using key only (no deserialization needed)
                if row_key.seq() <= cutoff_seq {
                    expired_keys.push(row_key);
                    if expired_keys.len() >= batch_target {
                        break;
                    }
                } else {
                    // Found a fresh row for this user. Since we scan Newer (oldest first),
                    // all subsequent rows for this user are also fresh.
                    skip_current_user = true;
                }
            }

            if expired_keys.len() >= batch_target {
                break;
            }

            if batch_count < scan_chunk {
                break;
            }

            if let Some(last_key) = last_processed_key {
                start_key = Some(last_key);
            } else {
                break;
            }
        }

        if expired_keys.is_empty() {
            ctx.log_info("No expired rows found; nothing to evict");
            return Ok(JobDecision::Completed {
                message: Some(format!("No expired rows found in {}", table_id)),
            });
        }

        ctx.log_info(&format!(
            "Found {} expired rows ready for eviction (batch limit: {})",
            expired_keys.len(),
            batch_size
        ));

        // Delete expired rows in batch
        let mut deleted_count = 0;
        for key in &expired_keys {
            match store.delete(key) {
                Ok(_) => deleted_count += 1,
                Err(e) => {
                    ctx.log_warn(&format!("Failed to delete row {:?}: {}", key, e));
                }
            }
        }

        ctx.log_info(&format!(
            "Stream eviction completed - {} rows evicted from {}",
            deleted_count, table_id
        ));

        // If we hit batch size limit, schedule retry for next batch
        if expired_keys.len() >= batch_target {
            ctx.log_info("Batch size limit reached - scheduling retry for next batch");
            return Ok(JobDecision::Retry {
                message: format!("Batch evicted {} rows, more remaining", deleted_count),
                exception_trace: None,
                backoff_ms: 5000,
            });
        }

        Ok(JobDecision::Completed {
            message: Some(format!(
                "Evicted {} expired records from {} (ttl: {}s)",
                deleted_count, table_id, ttl_seconds
            )),
        })
    }

    async fn cancel(&self, ctx: &JobContext<Self::Params>) -> Result<(), KalamDbError> {
        ctx.log_warn("Stream eviction job cancellation requested");
        // Allow cancellation since partial eviction is acceptable
        Ok(())
    }
}

impl Default for StreamEvictionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;
    use crate::providers::arrow_json_conversion::json_to_row;
    use crate::providers::base::{BaseTableProvider, TableProviderCore};
    use crate::providers::StreamTableProvider;
    use crate::schema_registry::CachedTableData;
    use crate::test_helpers::init_test_app_context;
    use chrono::Utc;
    use datafusion::datasource::TableProvider;
    use kalamdb_commons::models::datatypes::KalamDataType;
    use kalamdb_commons::models::schemas::{
        ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
    };
    use kalamdb_commons::models::{TableId, TableName, UserId};
    use kalamdb_commons::system::Job;
    use kalamdb_commons::{JobId, NamespaceId, NodeId};
    use kalamdb_store::entity_store::EntityStore;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    fn make_job(id: &str, job_type: JobType, _ns: &str) -> Job {
        let now = chrono::Utc::now().timestamp_millis();
        Job {
            job_id: JobId::new(id),
            job_type,
            status: kalamdb_commons::JobStatus::Running,
            parameters: None,
            message: None,
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: now,
            updated_at: now,
            started_at: Some(now),
            finished_at: None,
            node_id: NodeId::from(1u64),
            queue: None,
            priority: None,
        }
    }

    struct StreamTestHarness {
        table_id: TableId,
        namespace: NamespaceId,
        table_name_value: String,
        provider: Arc<StreamTableProvider>,
    }

    fn setup_stream_table(app_ctx: &Arc<AppContext>) -> StreamTestHarness {
        let namespace = NamespaceId::new("chat_stream_jobs");
        let table_name_value = format!(
            "typing_events_{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let tbl = TableName::new(&table_name_value);
        let table_id = TableId::new(namespace.clone(), tbl.clone());

        let table_def = TableDefinition::new(
            namespace.clone(),
            tbl.clone(),
            TableType::Stream,
            vec![
                ColumnDefinition::new(
                    1,
                    "event_id".to_string(),
                    1,
                    KalamDataType::Text,
                    false,
                    false,
                    false,
                    ColumnDefault::None,
                    None,
                ),
                ColumnDefinition::new(
                    2,
                    "payload".to_string(),
                    2,
                    KalamDataType::Text,
                    false,
                    false,
                    false,
                    ColumnDefault::None,
                    None,
                ),
            ],
            TableOptions::stream(1),
            None,
        )
        .expect("table definition");

        app_ctx.schema_registry().insert(
            table_id.clone(),
            Arc::new(CachedTableData::new(Arc::new(table_def))),
        );

        let stream_store = Arc::new(kalamdb_tables::new_stream_table_store(&table_id));
        let core = Arc::new(TableProviderCore::from_app_context(
            app_ctx,
            table_id.clone(),
            TableType::Stream,
        ));
        let provider = Arc::new(StreamTableProvider::new(
            core,
            stream_store,
            Some(1),
            "event_id".to_string(),
        ));
        let provider_trait: Arc<dyn TableProvider> = provider.clone();
        app_ctx
            .schema_registry()
            .insert_provider(table_id.clone(), provider_trait)
            .expect("register provider");

        StreamTestHarness {
            table_id,
            namespace,
            table_name_value,
            provider,
        }
    }

    #[test]
    fn test_params_validation_success() {
        let params = StreamEvictionParams {
            table_id: TableId::new(
                NamespaceId::new("default"),
                kalamdb_commons::TableName::new("events"),
            ),
            table_type: TableType::Stream,
            ttl_seconds: 86400,
            batch_size: 10000,
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_params_validation_invalid_table_type() {
        let params = StreamEvictionParams {
            table_id: TableId::new(
                NamespaceId::new("default"),
                kalamdb_commons::TableName::new("events"),
            ),
            table_type: TableType::User, // Wrong type
            ttl_seconds: 86400,
            batch_size: 10000,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_params_validation_zero_ttl() {
        let params = StreamEvictionParams {
            table_id: TableId::new(
                NamespaceId::new("default"),
                kalamdb_commons::TableName::new("events"),
            ),
            table_type: TableType::Stream,
            ttl_seconds: 0, // Invalid
            batch_size: 10000,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_executor_properties() {
        let executor = StreamEvictionExecutor::new();
        assert_eq!(executor.job_type(), JobType::StreamEviction);
        assert_eq!(executor.name(), "StreamEvictionExecutor");
    }

    #[tokio::test]
    async fn test_execute_evicts_expired_rows_from_provider_store() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let harness = setup_stream_table(&app_ctx);
        let provider = harness.provider.clone();

        // Insert a couple of rows
        let user = UserId::new("user-ttl");
        provider
            .insert(
                &user,
                json_to_row(&json!({"event_id": "evt1", "payload": "hello"})).unwrap(),
            )
            .expect("insert evt1");
        provider
            .insert(
                &user,
                json_to_row(&json!({"event_id": "evt2", "payload": "world"})).unwrap(),
            )
            .expect("insert evt2");

        // Wait for TTL to make them eligible for eviction
        // Need to wait longer than TTL to ensure Snowflake timestamp is old enough
        sleep(Duration::from_millis(1500)).await;

        let mut job = make_job(
            "SE-evict",
            JobType::StreamEviction,
            harness.namespace.as_str(),
        );
        job.parameters = Some(
            json!({
                "namespace_id": harness.namespace.as_str(),
                "table_name": harness.table_name_value.clone(),
                "table_type": "Stream",
                "ttl_seconds": 1,
                "batch_size": 100
            })
            .to_string(),
        );

        let params = StreamEvictionParams {
            table_id: harness.table_id.clone(),
            table_type: TableType::Stream,
            ttl_seconds: 1,
            batch_size: 100,
        };

        let ctx = JobContext::new(app_ctx.clone(), job.job_id.as_str().to_string(), params);
        let executor = StreamEvictionExecutor::new();
        let decision = executor.execute(&ctx).await.expect("execute eviction");

        match decision {
            JobDecision::Completed { message } => {
                assert!(message.unwrap().contains("Evicted"));
            }
            other => panic!("Expected Completed decision, got {:?}", other),
        }

        let remaining_iter = harness
            .provider
            .store_arc()
            .scan_iterator(None, None)
            .expect("scan store after eviction");
        
        let remaining_count = remaining_iter.count();
        assert_eq!(remaining_count, 0, "All expired rows should be removed");
    }

    #[tokio::test]
    async fn test_pre_validate_skips_when_no_expired_rows() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let harness = setup_stream_table(&app_ctx);

        let user = UserId::new("user-prevalidate-no-expired");
        harness
            .provider
            .insert(
                &user,
                json_to_row(&json!({"event_id": "evt1", "payload": "fresh"})).unwrap(),
            )
            .expect("insert fresh row");

        let params = StreamEvictionParams {
            table_id: harness.table_id.clone(),
            table_type: TableType::Stream,
            ttl_seconds: 60,
            batch_size: 100,
        };

        let executor = StreamEvictionExecutor::new();
        let should_run = executor
            .pre_validate(&app_ctx, &params)
            .await
            .expect("pre_validate execution");

        assert!(!should_run, "No expired rows should skip job creation");
    }

    #[tokio::test]
    async fn test_pre_validate_detects_expired_rows() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let harness = setup_stream_table(&app_ctx);

        let user = UserId::new("user-prevalidate-expired");
        harness
            .provider
            .insert(
                &user,
                json_to_row(&json!({"event_id": "evt1", "payload": "expired"})).unwrap(),
            )
            .expect("insert expired row");

        sleep(Duration::from_millis(1200)).await;

        let params = StreamEvictionParams {
            table_id: harness.table_id.clone(),
            table_type: TableType::Stream,
            ttl_seconds: 1,
            batch_size: 10,
        };

        let executor = StreamEvictionExecutor::new();
        let should_run = executor
            .pre_validate(&app_ctx, &params)
            .await
            .expect("pre_validate execution");

        assert!(should_run, "Expired rows should trigger job creation");
    }
}
