// backend/crates/kalamdb-live/src/initial_data.rs
//
// Initial data fetch for live query subscriptions.
// Provides "changes since timestamp" functionality to populate client state
// before real-time notifications begin.

use super::filter::FilterPredicate;
use crate::error::KalamDbError;
use crate::schema_registry::TableType;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::SqlExecutor;
use datafusion::execution::context::SessionContext;
use kalamdb_commons::constants::{AuthConstants, SystemColumnNames};
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::row::Row;
use kalamdb_commons::models::{TableId, UserId};
use kalamdb_commons::Role;
use once_cell::sync::OnceCell;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Options for fetching initial data when subscribing to a live query
#[derive(Debug, Clone)]
pub struct InitialDataOptions {
    /// Fetch changes since this sequence ID (exclusive)
    /// If None, starts from the beginning (or end, depending on strategy)
    pub since_seq: Option<SeqId>,

    /// Fetch changes up to this sequence ID (inclusive)
    /// Used to define the snapshot boundary
    pub until_seq: Option<SeqId>,

    /// Maximum number of rows to return (batch size)
    /// Default: 100
    pub limit: usize,

    /// Include soft-deleted rows (_deleted=true)
    /// Default: false
    pub include_deleted: bool,

    /// Fetch the last N rows (newest first) instead of from the beginning
    /// Default: false
    pub fetch_last: bool,
}

impl Default for InitialDataOptions {
    fn default() -> Self {
        Self {
            since_seq: None,
            until_seq: None,
            limit: 100,
            include_deleted: false,
            fetch_last: false,
        }
    }
}

impl InitialDataOptions {
    /// Create options to fetch changes since a specific sequence ID
    pub fn since(seq: SeqId) -> Self {
        Self {
            since_seq: Some(seq),
            until_seq: None,
            limit: 100,
            include_deleted: false,
            fetch_last: false,
        }
    }

    /// Create options to fetch the last N rows (legacy/simple mode)
    /// Note: This might need adjustment for SeqId-based logic
    pub fn last(limit: usize) -> Self {
        Self {
            since_seq: None,
            until_seq: None,
            limit,
            include_deleted: false,
            fetch_last: true,
        }
    }

    /// Create options for batch-based fetching
    pub fn batch(since_seq: Option<SeqId>, until_seq: Option<SeqId>, batch_size: usize) -> Self {
        Self {
            since_seq,
            until_seq,
            limit: batch_size,
            include_deleted: false,
            fetch_last: false,
        }
    }

    /// Set the maximum number of rows to return
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Include soft-deleted rows in the result
    pub fn with_deleted(mut self) -> Self {
        self.include_deleted = true;
        self
    }
}

/// Result of an initial data fetch
#[derive(Debug, Clone)]
pub struct InitialDataResult {
    /// The fetched rows (as Row objects)
    pub rows: Vec<Row>,

    /// Sequence ID of the last row in the result
    /// Used for pagination (passed as since_seq in next request)
    pub last_seq: Option<SeqId>,

    /// Whether there are more rows available in the snapshot range
    pub has_more: bool,

    /// The snapshot boundary used for this fetch
    pub snapshot_end_seq: Option<SeqId>,
}

/// Service for fetching initial data when subscribing to live queries
pub struct InitialDataFetcher {
    base_session_context: Arc<SessionContext>,
    schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
    sql_executor: OnceCell<Arc<SqlExecutor>>,
}

impl InitialDataFetcher {
    /// Create a new initial data fetcher
    pub fn new(
        base_session_context: Arc<SessionContext>,
        schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
    ) -> Self {
        Self {
            base_session_context,
            schema_registry,
            sql_executor: OnceCell::new(),
        }
    }

    /// Wire up the shared SqlExecutor so all queries reuse the same execution path
    pub fn set_sql_executor(&self, executor: Arc<SqlExecutor>) {
        if self.sql_executor.set(executor).is_err() {
            log::warn!(
                "SqlExecutor already set for InitialDataFetcher; ignoring duplicate assignment"
            );
        }
    }

    /// Fetch initial data for a table
    ///
    /// # Arguments
    /// * `table_id` - Table identifier with namespace and table name
    /// * `table_type` - User or Shared table
    /// * `options` - Options for the fetch (timestamp, limit, etc.)
    ///
    /// # Returns
    /// InitialDataResult with rows and metadata
    pub async fn fetch_initial_data(
        &self,
        live_id: &super::connection_registry::LiveId,
        table_id: &TableId,
        table_type: TableType,
        options: InitialDataOptions,
        filter: Option<Arc<FilterPredicate>>,
    ) -> Result<InitialDataResult, KalamDbError> {
        log::info!(
            "fetch_initial_data called: table={}, type={:?}, limit={}, since={:?}",
            table_id,
            table_type,
            options.limit,
            options.since_seq
        );

        let limit = options.limit;
        if limit == 0 {
            log::debug!("Limit is 0, returning empty result");
            return Ok(InitialDataResult {
                rows: Vec::new(),
                last_seq: None,
                has_more: false,
                snapshot_end_seq: None,
            });
        }

        // Extract user_id from LiveId for RLS
        let user_id = UserId::new(live_id.user_id().to_string());

        // Determine role based on user_id
        let role = if user_id.as_str() == AuthConstants::DEFAULT_ROOT_USER_ID {
            Role::System
        } else {
            Role::User
        };

        let sql_executor = self.sql_executor.get().cloned().ok_or_else(|| {
            KalamDbError::InvalidOperation(
                "SqlExecutor not configured for InitialDataFetcher".to_string(),
            )
        })?;

        // Create execution context with user scope for row-level security
        let exec_ctx = ExecutionContext::new(
            user_id.clone(),
            role,
            Arc::clone(&self.base_session_context),
        );

        // Construct SQL query
        let table_name = format!(
            "{}.{}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );
        let mut sql = format!("SELECT * FROM {}", table_name);

        let mut where_clauses = Vec::new();

        // Add _seq filters
        if let Some(since) = options.since_seq {
            where_clauses.push(format!("{} > {}", SystemColumnNames::SEQ, since.as_i64()));
        }
        if let Some(until) = options.until_seq {
            where_clauses.push(format!("{} <= {}", SystemColumnNames::SEQ, until.as_i64()));
        }

        // Add deleted filter
        if !options.include_deleted {
            if matches!(table_type, TableType::User | TableType::Shared)
                && self.table_has_column(table_id, SystemColumnNames::DELETED)?
            {
                where_clauses.push(format!("{} = false", SystemColumnNames::DELETED));
            }
        }

        // Add custom filter from subscription
        if let Some(predicate) = filter {
            where_clauses.push(predicate.sql().to_string());
        }

        if !where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }

        // Add ORDER BY
        if options.fetch_last {
            sql.push_str(&format!(" ORDER BY {} DESC", SystemColumnNames::SEQ));
        } else {
            sql.push_str(&format!(" ORDER BY {} ASC", SystemColumnNames::SEQ));
        }

        // Add LIMIT (fetch limit + 1 to check has_more)
        sql.push_str(&format!(" LIMIT {}", limit + 1));

        log::debug!("Executing initial data SQL via SqlExecutor: {}", sql);

        let execution_result = sql_executor
            .execute(&sql, &exec_ctx, Vec::<ScalarValue>::new())
            .await?;

        let batches = match execution_result {
            ExecutionResult::Rows { batches, .. } => batches,
            other => {
                return Err(KalamDbError::ExecutionError(format!(
                    "Initial data query returned non-row result: {:?}",
                    other
                )))
            }
        };

        // Convert batches to Rows
        let mut rows_with_seq: Vec<(SeqId, Row)> = Vec::new();

        for batch in batches {
            let schema = batch.schema();
            let seq_col_idx = schema
                .index_of(SystemColumnNames::SEQ)
                .map_err(|_| KalamDbError::Other(format!("Result missing {} column", SystemColumnNames::SEQ)))?;

            let seq_col = batch.column(seq_col_idx);
            let seq_array = seq_col
                .as_any()
                .downcast_ref::<datafusion::arrow::array::Int64Array>()
                .ok_or_else(|| KalamDbError::Other(format!("{} column is not Int64", SystemColumnNames::SEQ)))?;

            let num_rows = batch.num_rows();
            let num_cols = batch.num_columns();

            for row_idx in 0..num_rows {
                let mut row_map = BTreeMap::new();
                for col_idx in 0..num_cols {
                    let col_name = schema.field(col_idx).name();
                    let col_array = batch.column(col_idx);
                    let value = ScalarValue::try_from_array(col_array, row_idx).map_err(|e| {
                        KalamDbError::SerializationError(format!(
                            "Failed to convert to ScalarValue: {}",
                            e
                        ))
                    })?;
                    row_map.insert(col_name.clone(), value);
                }

                let seq_val = seq_array.value(row_idx);
                let seq_id = SeqId::from(seq_val);
                rows_with_seq.push((seq_id, Row::new(row_map)));
            }
        }

        // Determine has_more and slice to limit
        let total_fetched = rows_with_seq.len();
        let has_more = total_fetched > limit;

        let mut batch_rows = if has_more {
            rows_with_seq.into_iter().take(limit).collect::<Vec<_>>()
        } else {
            rows_with_seq
        };

        // If we fetched last rows (DESC), we need to reverse them to return in chronological order
        if options.fetch_last {
            batch_rows.reverse();
        }

        // Determine snapshot boundary
        let last_seq = batch_rows.last().map(|(seq, _)| *seq);
        let snapshot_end_seq = options.until_seq.or(last_seq);

        let rows: Vec<Row> = batch_rows.into_iter().map(|(_, row)| row).collect();

        log::info!(
            "fetch_initial_data complete: table={}, returned {} rows (has_more: {}, last_seq: {:?})",
            table_id,
            rows.len(),
            has_more,
            last_seq
        );

        Ok(InitialDataResult {
            rows,
            last_seq,
            has_more,
            snapshot_end_seq,
        })
    }

    fn table_has_column(
        &self,
        table_id: &TableId,
        column_name: &str,
    ) -> Result<bool, KalamDbError> {
        let schema = self.schema_registry.get_arrow_schema(table_id)?;
        Ok(schema.field_with_name(column_name).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::arrow_json_conversion::json_to_row;
    use crate::sql::executor::SqlExecutor;
    use kalamdb_commons::ids::{SeqId, UserTableRowId};
    use kalamdb_commons::models::{ConnectionId as ConnId, LiveId as CommonsLiveId};
    use kalamdb_commons::models::{NamespaceId, TableName};
    use kalamdb_commons::UserId;
    use kalamdb_store::entity_store::EntityStore;
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_tables::user_tables::user_table_store::{new_user_table_store, UserTableRow};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    static INITIAL_DATA_TEST_GUARD: once_cell::sync::Lazy<Mutex<()>> =
        once_cell::sync::Lazy::new(|| Mutex::new(()));

    #[test]
    fn test_initial_data_options_default() {
        let options = InitialDataOptions::default();
        assert_eq!(options.since_seq, None);
        assert_eq!(options.limit, 100);
        assert!(!options.include_deleted);
        assert!(!options.fetch_last);
    }

    #[test]
    fn test_initial_data_options_since() {
        let seq = SeqId::new(12345);
        let options = InitialDataOptions::since(seq);
        assert_eq!(options.since_seq, Some(seq));
        assert_eq!(options.limit, 100);
        assert!(!options.include_deleted);
        assert!(!options.fetch_last);
    }

    #[test]
    fn test_initial_data_options_last() {
        let options = InitialDataOptions::last(50);
        assert_eq!(options.since_seq, None);
        assert_eq!(options.limit, 50);
        assert!(!options.include_deleted);
        assert!(options.fetch_last);
    }

    #[test]
    fn test_initial_data_options_builder() {
        let seq = SeqId::new(12345);
        let options = InitialDataOptions::since(seq)
            .with_limit(200)
            .with_deleted();

        assert_eq!(options.since_seq, Some(seq));
        assert_eq!(options.limit, 200);
        assert!(options.include_deleted);
    }

    #[tokio::test]
    async fn test_user_table_initial_fetch_returns_rows() {
        let _guard = INITIAL_DATA_TEST_GUARD.lock().await;
        // Initialize global AppContext for the test (idempotent)
        let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(InMemoryBackend::new());
        let node_id = kalamdb_commons::NodeId::new("test-node".to_string());
        let config = kalamdb_commons::ServerConfig::default();

        // We use init() which uses get_or_init() internally, so it's safe to call multiple times
        // (subsequent calls return the existing instance)
        let app_context = crate::app_context::AppContext::init(
            backend.clone(),
            node_id,
            "/tmp/kalamdb-test".to_string(),
            config,
        );

        let schema_registry = app_context.schema_registry();

        // Setup in-memory user table with one row (userA)
        // For User tables, namespace must match user_id
        let user_id = UserId::from("usera");
        let ns = NamespaceId::new(user_id.as_str());
        let tbl = TableName::new("items");
        let table_id = kalamdb_commons::models::TableId::new(ns.clone(), tbl.clone());
        let store = Arc::new(new_user_table_store(backend.clone(), &ns, &tbl));

        let seq = SeqId::new(1234567890);
        let row_id = UserTableRowId::new(user_id.clone(), seq);

        let row = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq,
            fields: json_to_row(&serde_json::json!({"id": 1, "name": "Item One"})).unwrap(),
            _deleted: false,
        };

        // Insert row using EntityStore trait
        EntityStore::put(&*store, &row_id, &row).expect("put row");

        // Register table definition so get_arrow_schema works
        use crate::schema_registry::CachedTableData;
        use kalamdb_commons::models::datatypes::KalamDataType;
        use kalamdb_commons::models::schemas::column_default::ColumnDefault;
        use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};

        let columns = vec![
            ColumnDefinition::primary_key("id", 1, KalamDataType::Int),
            ColumnDefinition::simple("name", 2, KalamDataType::Text),
            ColumnDefinition::new(
                "_seq",
                3,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                "_deleted",
                4,
                KalamDataType::Boolean,
                false,
                false,
                false,
                ColumnDefault::literal(serde_json::json!(false)),
                None,
            ),
        ];

        let table_def = TableDefinition::new_with_defaults(
            ns.clone(),
            tbl.clone(),
            kalamdb_commons::TableType::User,
            columns,
            None,
        )
        .expect("create table def");

        // Insert into cache directly (bypassing persistence which might not be mocked)
        schema_registry.insert(
            table_id.clone(),
            Arc::new(CachedTableData::new(Arc::new(table_def))),
        );

        // Create a mock provider with the store
        use crate::providers::base::TableProviderCore;
        use crate::providers::UserTableProvider;
        let core = Arc::new(TableProviderCore::from_app_context(
            &app_context,
            table_id.clone(),
            TableType::User,
        ));
        let provider = Arc::new(UserTableProvider::new(core, store, "id".to_string()));

        // Register the provider in schema_registry
        schema_registry
            .insert_provider(table_id.clone(), provider)
            .expect("register provider");

        let fetcher =
            InitialDataFetcher::new(app_context.base_session_context(), schema_registry.clone());
        let sql_executor = Arc::new(SqlExecutor::new(app_context.clone(), false));
        fetcher.set_sql_executor(sql_executor);

        // LiveId for connection user 'userA' (RLS enforced)
        let conn = ConnId::new("usera".to_string(), "conn1".to_string());
        let live = CommonsLiveId::new(conn, table_id.clone(), "q1".to_string());

        // Fetch initial data (default options: last 100)
        let res = fetcher
            .fetch_initial_data(
                &live,
                &table_id,
                TableType::User,
                InitialDataOptions::last(100),
                None,
            )
            .await
            .expect("initial fetch");

        assert_eq!(res.rows.len(), 1, "Expected one row in initial snapshot");

        // Verify the row content
        let row = &res.rows[0];
        assert_eq!(row.get("id").unwrap(), &ScalarValue::Int32(Some(1)));
        assert_eq!(
            row.get("name").unwrap(),
            &ScalarValue::Utf8(Some("Item One".to_string()))
        );
    }

    #[tokio::test]
    async fn test_user_table_batch_fetching() {
        let _guard = INITIAL_DATA_TEST_GUARD.lock().await;
        // Initialize global AppContext for the test (idempotent)
        let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(InMemoryBackend::new());
        let node_id = kalamdb_commons::NodeId::new("test-node-batch".to_string());
        let config = kalamdb_commons::ServerConfig::default();

        let app_context = crate::app_context::AppContext::init(
            backend.clone(),
            node_id,
            "/tmp/kalamdb-test-batch".to_string(),
            config,
        );

        let schema_registry = app_context.schema_registry();

        // Setup in-memory user table
        // For User tables, namespace must match user_id
        let user_id = UserId::from("userb");
        let ns = NamespaceId::new(user_id.as_str());
        let tbl = TableName::new("batch_items");
        let table_id = kalamdb_commons::models::TableId::new(ns.clone(), tbl.clone());
        let store = Arc::new(new_user_table_store(backend.clone(), &ns, &tbl));

        // Insert 3 rows with increasing seq
        for i in 1..=3 {
            let seq = SeqId::new(i);
            let row_id = UserTableRowId::new(user_id.clone(), seq);
            let fields =
                json_to_row(&serde_json::json!({"id": i, "val": format!("Item {}", i)})).unwrap();

            let row = UserTableRow {
                user_id: user_id.clone(),
                _seq: seq,
                fields,
                _deleted: false,
            };

            EntityStore::put(&*store, &row_id, &row).expect("put row");
        }

        // Register table definition
        use crate::schema_registry::CachedTableData;
        use kalamdb_commons::models::datatypes::KalamDataType;
        use kalamdb_commons::models::schemas::column_default::ColumnDefault;
        use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};

        let columns = vec![
            ColumnDefinition::primary_key("id", 1, KalamDataType::Int),
            ColumnDefinition::simple("val", 2, KalamDataType::Text),
            ColumnDefinition::new(
                "_seq",
                3,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                "_deleted",
                4,
                KalamDataType::Boolean,
                false,
                false,
                false,
                ColumnDefault::literal(serde_json::json!(false)),
                None,
            ),
        ];

        let table_def = TableDefinition::new_with_defaults(
            ns.clone(),
            tbl.clone(),
            kalamdb_commons::TableType::User,
            columns,
            None,
        )
        .expect("create table def");

        schema_registry.insert(
            table_id.clone(),
            Arc::new(CachedTableData::new(Arc::new(table_def))),
        );

        // Create and register provider
        use crate::providers::base::TableProviderCore;
        use crate::providers::UserTableProvider;
        let core = Arc::new(TableProviderCore::from_app_context(
            &app_context,
            table_id.clone(),
            TableType::User,
        ));
        let provider = Arc::new(UserTableProvider::new(core, store, "id".to_string()));

        schema_registry
            .insert_provider(table_id.clone(), provider)
            .expect("register provider");

        let fetcher =
            InitialDataFetcher::new(app_context.base_session_context(), schema_registry.clone());
        let sql_executor = Arc::new(SqlExecutor::new(app_context.clone(), false));
        fetcher.set_sql_executor(sql_executor);
        let conn = ConnId::new("userb".to_string(), "conn2".to_string());
        let live = CommonsLiveId::new(conn, table_id.clone(), "q2".to_string());

        // 1. Fetch first batch (limit 1)
        let res1 = fetcher
            .fetch_initial_data(
                &live,
                &table_id,
                TableType::User,
                InitialDataOptions::default().with_limit(1),
                None,
            )
            .await
            .expect("fetch batch 1");

        assert_eq!(res1.rows.len(), 1);
        assert_eq!(
            res1.rows[0].get("id").unwrap(),
            &ScalarValue::Int32(Some(1))
        );
        assert!(res1.has_more);
        assert_eq!(res1.last_seq, Some(SeqId::new(1)));

        // 2. Fetch second batch (since_seq=1, limit 1)
        let res2 = fetcher
            .fetch_initial_data(
                &live,
                &table_id,
                TableType::User,
                InitialDataOptions::since(res1.last_seq.unwrap()).with_limit(1),
                None,
            )
            .await
            .expect("fetch batch 2");

        assert_eq!(res2.rows.len(), 1);
        assert_eq!(
            res2.rows[0].get("id").unwrap(),
            &ScalarValue::Int32(Some(2))
        );
        assert!(res2.has_more);
        assert_eq!(res2.last_seq, Some(SeqId::new(2)));

        // 3. Fetch third batch (since_seq=2, limit 1)
        let res3 = fetcher
            .fetch_initial_data(
                &live,
                &table_id,
                TableType::User,
                InitialDataOptions::since(res2.last_seq.unwrap()).with_limit(1),
                None,
            )
            .await
            .expect("fetch batch 3");

        assert_eq!(res3.rows.len(), 1);
        assert_eq!(
            res3.rows[0].get("id").unwrap(),
            &ScalarValue::Int32(Some(3))
        );
        assert!(!res3.has_more); // Should be false as we fetched the last one
        assert_eq!(res3.last_seq, Some(SeqId::new(3)));
    }

    #[tokio::test]
    async fn test_user_table_fetch_last_rows() {
        let _guard = INITIAL_DATA_TEST_GUARD.lock().await;
        // Initialize global AppContext for the test (idempotent)
        let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(InMemoryBackend::new());
        let node_id = kalamdb_commons::NodeId::new("test-node-last".to_string());
        let config = kalamdb_commons::ServerConfig::default();

        let app_context = crate::app_context::AppContext::init(
            backend.clone(),
            node_id,
            "/tmp/kalamdb-test-last".to_string(),
            config,
        );

        let schema_registry = app_context.schema_registry();

        // Setup in-memory user table
        // For User tables, namespace must match user_id
        let user_id = UserId::from("userc");
        let ns = NamespaceId::new(user_id.as_str());
        let tbl = TableName::new("last_items");
        let table_id = kalamdb_commons::models::TableId::new(ns.clone(), tbl.clone());
        let store = Arc::new(new_user_table_store(backend.clone(), &ns, &tbl));

        // Insert 10 rows with increasing seq
        for i in 1..=10 {
            let seq = SeqId::new(i);
            let row_id = UserTableRowId::new(user_id.clone(), seq);
            let fields =
                json_to_row(&serde_json::json!({"id": i, "val": format!("Item {}", i)})).unwrap();

            let row = UserTableRow {
                user_id: user_id.clone(),
                _seq: seq,
                fields,
                _deleted: false,
            };

            EntityStore::put(&*store, &row_id, &row).expect("put row");
        }

        // Register table definition
        use crate::schema_registry::CachedTableData;
        use kalamdb_commons::models::datatypes::KalamDataType;
        use kalamdb_commons::models::schemas::column_default::ColumnDefault;
        use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};

        let columns = vec![
            ColumnDefinition::primary_key("id", 1, KalamDataType::Int),
            ColumnDefinition::simple("val", 2, KalamDataType::Text),
            ColumnDefinition::new(
                "_seq",
                3,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                "_deleted",
                4,
                KalamDataType::Boolean,
                false,
                false,
                false,
                ColumnDefault::literal(serde_json::json!(false)),
                None,
            ),
        ];

        let table_def = TableDefinition::new_with_defaults(
            ns.clone(),
            tbl.clone(),
            kalamdb_commons::TableType::User,
            columns,
            None,
        )
        .expect("create table def");

        schema_registry.insert(
            table_id.clone(),
            Arc::new(CachedTableData::new(Arc::new(table_def))),
        );

        // Create and register provider
        use crate::providers::base::TableProviderCore;
        use crate::providers::UserTableProvider;
        let core = Arc::new(TableProviderCore::from_app_context(
            &app_context,
            table_id.clone(),
            TableType::User,
        ));
        let provider = Arc::new(UserTableProvider::new(core, store, "id".to_string()));

        schema_registry
            .insert_provider(table_id.clone(), provider)
            .expect("register provider");

        let fetcher =
            InitialDataFetcher::new(app_context.base_session_context(), schema_registry.clone());
        let sql_executor = Arc::new(SqlExecutor::new(app_context.clone(), false));
        fetcher.set_sql_executor(sql_executor);
        let conn = ConnId::new("userc".to_string(), "conn3".to_string());
        let live = CommonsLiveId::new(conn, table_id.clone(), "q3".to_string());

        // Fetch last 3 rows
        let res = fetcher
            .fetch_initial_data(
                &live,
                &table_id,
                TableType::User,
                InitialDataOptions::last(3),
                None,
            )
            .await
            .expect("fetch last 3");

        assert_eq!(res.rows.len(), 3);
        // Should be 8, 9, 10 in that order
        assert_eq!(res.rows[0].get("id").unwrap(), &ScalarValue::Int32(Some(8)));
        assert_eq!(res.rows[1].get("id").unwrap(), &ScalarValue::Int32(Some(9)));
        assert_eq!(
            res.rows[2].get("id").unwrap(),
            &ScalarValue::Int32(Some(10))
        );

        assert!(res.has_more);
    }
}
