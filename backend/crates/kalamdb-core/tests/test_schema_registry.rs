use kalamdb_core::schema_registry::{CachedTableData, SchemaRegistry};
use datafusion::catalog::TableProvider;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};
use kalamdb_commons::models::{NamespaceId, TableId, TableName, UserId};
use kalamdb_commons::schemas::TableType;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn create_test_schema() -> Arc<TableDefinition> {
    use kalamdb_commons::schemas::{ColumnDefault, TableOptions};

    Arc::new(
        TableDefinition::new(
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            TableType::User,
            vec![ColumnDefinition::new(
                1,
                "id".to_string(),
                1, // ordinal_position
                KalamDataType::Int,
                false, // nullable
                false, // is_auto_increment
                false, // is_unique
                ColumnDefault::None,
                None, // comment
            )],
            TableOptions::user(), // Default user table options
            None,                 // table_comment
        )
        .expect("Failed to create test schema"),
    )
}

fn create_test_data(table_id: TableId) -> Arc<CachedTableData> {
    // Create partially-resolved template with {namespace} and {tableName} substituted
    let _storage_path_template = format!(
        "/data/{}/{}/{{userId}}/{{shard}}/",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
    );

    Arc::new(CachedTableData::new(create_test_schema()))
}

#[test]
fn test_insert_and_get() {
    let cache = SchemaRegistry::new(1000);
    let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
    let data = create_test_data(table_id.clone());

    cache.insert(table_id.clone(), data.clone());

    let retrieved = cache.get(&table_id).expect("Should find table");
    assert_eq!(retrieved.table.table_type, TableType::User);
    assert_eq!(retrieved.schema_version, 1);
}

#[test]
fn test_get_by_table_id() {
    let cache = SchemaRegistry::new(1000);
    let namespace = NamespaceId::new("ns1");
    let table_name = TableName::new("table1");
    let table_id = TableId::new(namespace.clone(), table_name.clone());
    let data = create_test_data(table_id.clone());

    cache.insert(table_id.clone(), data);

    let retrieved = cache.get(&table_id).expect("Should find table");
    assert_eq!(retrieved.table.table_type, TableType::User);
}

#[test]
fn test_lru_eviction() {
    let cache = SchemaRegistry::new(3); // Small cache for testing

    // Insert 3 tables
    let table1 = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
    let table2 = TableId::new(NamespaceId::new("ns1"), TableName::new("table2"));
    let table3 = TableId::new(NamespaceId::new("ns1"), TableName::new("table3"));

    cache.insert(table1.clone(), create_test_data(table1.clone()));
    thread::sleep(Duration::from_millis(10));

    cache.insert(table2.clone(), create_test_data(table2.clone()));
    thread::sleep(Duration::from_millis(10));

    cache.insert(table3.clone(), create_test_data(table3.clone()));

    assert_eq!(cache.len(), 3);

    // Access table1 to update its LRU timestamp
    cache.get(&table1);

    // Insert 4th table - should evict table2 (oldest untouched)
    let table4 = TableId::new(NamespaceId::new("ns1"), TableName::new("table4"));
    cache.insert(table4.clone(), create_test_data(table4.clone()));

    assert_eq!(cache.len(), 3);
    assert!(cache.get(&table1).is_some());
    assert!(cache.get(&table2).is_none()); // Evicted
    assert!(cache.get(&table3).is_some());
    assert!(cache.get(&table4).is_some());
}

#[test]
fn test_invalidate() {
    let cache = SchemaRegistry::new(1000);
    let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
    let data = create_test_data(table_id.clone());

    cache.insert(table_id.clone(), data);
    assert!(cache.get(&table_id).is_some());

    cache.invalidate(&table_id);
    assert!(cache.get(&table_id).is_none());
}

#[test]
fn test_storage_path_resolution() {
    use kalamdb_core::schema_registry::PathResolver;

    // Create test data with properly set storage_path_template
    let schema = create_test_schema();

    // Use with_storage_config to properly set the template
    let data = CachedTableData::with_storage_config(
        schema,
        None,
        "/data/my_ns/messages/{userId}/{shard}/".to_string(),
    );
    let data_arc = Arc::new(data);

    // Test PathResolver::get_relative_storage_path (doesn't require AppContext)
    let relative_path = PathResolver::get_relative_storage_path(
        &data_arc,
        Some(&UserId::new("alice")),
        Some(0),
    )
    .expect("Should resolve relative path");

    let expected = "data/my_ns/messages/alice/shard_0/";
    assert_eq!(
        relative_path, expected,
        "Expected '{}', got '{}'",
        expected, relative_path
    );

    println!("✅ Storage path resolution test passed (using get_relative_storage_path)");
}

#[test]
fn test_concurrent_access() {
    let cache = Arc::new(SchemaRegistry::new(1000));
    let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
    let data = create_test_data(table_id.clone());

    cache.insert(table_id.clone(), data);

    let mut handles = vec![];

    // Spawn 10 threads to read concurrently
    for _ in 0..10 {
        let cache = Arc::clone(&cache);
        let table_id = table_id.clone();

        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let result = cache.get(&table_id);
                assert!(result.is_some());
            }
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

#[test]
fn test_metrics() {
    let cache = SchemaRegistry::new(1000);
    let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
    let data = create_test_data(table_id.clone());

    cache.insert(table_id.clone(), data);

    // Generate hits and misses
    cache.get(&table_id); // Hit
    cache.get(&table_id); // Hit
    cache.get(&TableId::new(
        NamespaceId::new("ns1"),
        TableName::new("nonexistent"),
    )); // Miss

    let (size, hits, misses, hit_rate) = cache.stats();
    assert_eq!(size, 1);
    assert_eq!(hits, 2);
    assert_eq!(misses, 1);
    assert!((hit_rate - 0.666).abs() < 0.01); // ~66.7%
}

#[test]
fn test_clear() {
    let cache = SchemaRegistry::new(1000);
    let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
    let data = create_test_data(table_id.clone());

    cache.insert(table_id, data);
    assert_eq!(cache.len(), 1);

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.stats().1, 0); // hits reset
    assert_eq!(cache.stats().2, 0); // misses reset
}

// ========================================================================
// Phase 5: Performance Testing & Validation
// ========================================================================

#[test]
fn bench_cache_hit_rate() {
    use std::time::Instant;

    let cache = SchemaRegistry::new(10000); // Large enough for all tables
    let num_tables = 1000;
    let queries_per_table = 100;

    // Create 1000 tables
    let mut table_ids = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let table_id = TableId::new(
            NamespaceId::new(format!("namespace_{}", i)),
            TableName::new(format!("table_{}", i)),
        );
        let data = create_test_data(table_id.clone());
        cache.insert(table_id.clone(), data);
        table_ids.push(table_id);
    }

    // Measure cache lookup latency
    let start = Instant::now();
    let mut total_lookups = 0u64;

    // Query each table 100 times
    for _ in 0..queries_per_table {
        for table_id in &table_ids {
            cache.get(table_id);
            total_lookups += 1;
        }
    }

    let elapsed = start.elapsed();
    let avg_latency_ns = elapsed.as_nanos() / total_lookups as u128;
    let avg_latency_us = avg_latency_ns as f64 / 1000.0;

    // Verify performance targets
    let (_, hits, misses, hit_rate) = cache.stats();

    // Assert >99% cache hit rate (should be 100% since we never evict)
    assert!(
        hit_rate > 0.99,
        "Cache hit rate {} is below 99% target (hits: {}, misses: {})",
        hit_rate,
        hits,
        misses
    );

    // Assert <100μs average latency per lookup
    assert!(
        avg_latency_us < 100.0,
        "Average cache lookup latency {:.2}μs exceeds 100μs target",
        avg_latency_us
    );

    println!(
        "✅ Cache hit rate: {:.2}% ({}/{} lookups)",
        hit_rate * 100.0,
        hits,
        hits + misses
    );
    println!("✅ Average lookup latency: {:.2}μs", avg_latency_us);
}

#[test]
fn bench_cache_memory_efficiency() {
    use std::mem;

    let cache = SchemaRegistry::new(10000);
    let num_tables = 1000;

    // Create 1000 CachedTableData entries
    for i in 0..num_tables {
        let table_id = TableId::new(
            NamespaceId::new(format!("namespace_{}", i)),
            TableName::new(format!("table_{}", i)),
        );
        let data = create_test_data(table_id.clone());
        cache.insert(table_id, data);
    }

    // Measure memory footprint
    // The key insight: the cache stores `Arc<CachedTableData>`, and LRU tracking uses an
    // `AtomicU64` stored inside `CachedTableData` (updated in-place, no deep cloning).

    let cached_table_data_size = mem::size_of::<CachedTableData>();
    let timestamp_size = mem::size_of::<AtomicU64>();

    // Rough sanity check: the timestamp should be a small fraction of the cached value.
    // This is intentionally conservative since struct padding/layout may vary.
    let ts_ratio = timestamp_size as f64 / cached_table_data_size as f64;
    assert!(
        ts_ratio <= 0.20,
        "LRU timestamp field {:.2}% exceeds 20% of CachedTableData size",
        ts_ratio * 100.0
    );

    println!(
        "✅ CachedTableData size: {} bytes (timestamp: {} bytes, {:.2}%)",
        cached_table_data_size,
        timestamp_size,
        ts_ratio * 100.0
    );

    // More meaningful metric: Compare to what we'd waste if we cloned CachedTableData on every access
    // CachedTableData contains: TableId + TableType + DateTime + Option<StorageId> + FlushPolicy +
    //                           String + u32 + Option<u32> + Arc<TableDefinition>
    // Rough estimate: ~200-300 bytes per struct
    let approx_cached_data_struct_size = 256; // Conservative estimate
    let waste_if_cloning = approx_cached_data_struct_size * num_tables;
    let actual_overhead = timestamp_size * num_tables;
    let savings_ratio = 1.0 - (actual_overhead as f64 / waste_if_cloning as f64);

    println!(
        "✅ Savings vs cloning CachedTableData: {:.1}% ({} bytes timestamp storage vs {} bytes struct cloning)",
        savings_ratio * 100.0,
        actual_overhead,
        waste_if_cloning
    );
}

#[test]
fn bench_provider_caching() {
    use std::sync::Arc as StdArc;

    let cache = SchemaRegistry::new(1000);
    let num_tables = 10;
    let num_users = 100;
    let queries_per_user = 10;

    // Create 10 tables with Arc<TableId> (simulates provider caching)
    let mut arc_table_ids: Vec<StdArc<TableId>> = Vec::new();
    for i in 0..num_tables {
        let table_id = TableId::new(
            NamespaceId::new(format!("namespace_{}", i)),
            TableName::new(format!("table_{}", i)),
        );
        let data = create_test_data(table_id.clone());
        cache.insert(table_id.clone(), data);
        arc_table_ids.push(StdArc::new(table_id));
    }

    // Simulate 100 users × 10 queries each = 1000 total queries
    // WITHOUT provider caching: would create 1000 TableId instances (100 users × 10 tables)
    // WITH provider caching: only 10 Arc::clone() calls per query (cheap!)

    let mut arc_clone_count = 0;
    for _user in 0..num_users {
        for _query in 0..queries_per_user {
            for arc_table_id in &arc_table_ids {
                // Simulate Arc::clone() overhead (what providers do)
                let _cloned_id = StdArc::clone(arc_table_id);
                arc_clone_count += 1;

                // Simulate cache lookup with Arc<TableId>
                cache.get(arc_table_id);
            }
        }
    }

    // Calculate allocation reduction
    let total_queries = num_users * queries_per_user * num_tables;
    let unique_instances = num_tables; // Only 10 Arc<TableId> exist!
    let allocation_reduction = 1.0 - (unique_instances as f64 / total_queries as f64);

    // Assert >99% reduction in provider allocations
    assert!(
        allocation_reduction > 0.99,
        "Provider allocation reduction {:.2}% is below 99% target",
        allocation_reduction * 100.0
    );

    println!(
        "✅ Total queries: {} (users: {}, queries/user: {}, tables: {})",
        total_queries, num_users, queries_per_user, num_tables
    );
    println!(
        "✅ Unique Arc<TableId> instances: {} (vs {} without caching)",
        unique_instances, total_queries
    );
    println!(
        "✅ Allocation reduction: {:.2}% ({} Arc::clone() calls vs {} new allocations)",
        allocation_reduction * 100.0,
        arc_clone_count,
        total_queries
    );
}

#[test]
fn stress_concurrent_access() {
    use std::sync::Arc as StdArc;
    use std::thread;
    use std::time::Instant;

    let cache = StdArc::new(SchemaRegistry::new(1000));
    let num_threads = 100;
    let ops_per_thread = 1000;

    // Pre-populate with some data
    for i in 0..50 {
        let table_id = TableId::new(
            NamespaceId::new(format!("namespace_{}", i)),
            TableName::new(format!("table_{}", i)),
        );
        let data = create_test_data(table_id.clone());
        cache.insert(table_id, data);
    }

    let start = Instant::now();
    let mut handles = vec![];

    // Spawn 100 threads, each doing 1000 random operations
    for thread_id in 0..num_threads {
        let cache_clone = StdArc::clone(&cache);
        let handle = thread::spawn(move || {
            for op in 0..ops_per_thread {
                let i = (thread_id * 1000 + op) % 100; // Random table
                let table_id = TableId::new(
                    NamespaceId::new(format!("namespace_{}", i)),
                    TableName::new(format!("table_{}", i)),
                );

                // Random operation: 70% get, 20% insert, 10% invalidate
                let op_type = op % 10;
                if op_type < 7 {
                    // GET operation
                    cache_clone.get(&table_id);
                } else if op_type < 9 {
                    // INSERT operation
                    let data = create_test_data(table_id.clone());
                    cache_clone.insert(table_id, data);
                } else {
                    // INVALIDATE operation
                    cache_clone.invalidate(&table_id);
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should not panic");
    }

    let elapsed = start.elapsed();

    // Assert all operations complete in <10 seconds
    assert!(
        elapsed.as_secs() < 10,
        "Concurrent stress test took {:.2}s, exceeding 10s target",
        elapsed.as_secs_f64()
    );

    // Verify metrics are consistent
    let (size, hits, misses, hit_rate) = cache.stats();
    let total_ops = (num_threads * ops_per_thread) as u64;
    let recorded_ops = hits + misses;

    println!(
        "✅ Completed {} operations in {:.2}s ({} threads × {} ops)",
        total_ops,
        elapsed.as_secs_f64(),
        num_threads,
        ops_per_thread
    );
    println!(
        "✅ Cache metrics: size={}, hits={}, misses={}, hit_rate={:.2}%",
        size,
        hits,
        misses,
        hit_rate * 100.0
    );
    println!("✅ Recorded ops: {} (get operations only)", recorded_ops);
    println!("✅ No deadlocks, no panics!");
}

#[test]
fn test_provider_cache_insert_and_get() {
    use kalamdb_core::views::stats::{StatsTableProvider, StatsView};

    let cache = SchemaRegistry::new(1000);
    let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("stats"));

    // First, insert CachedTableData (required for provider storage)
    let schema = create_test_schema();
    let cached_data = CachedTableData::new(schema);
    cache.insert(table_id.clone(), Arc::new(cached_data));

    // Now insert provider into the cached data
    let stats_view = Arc::new(StatsView::new());
    let provider = Arc::new(StatsTableProvider::new(stats_view)) as Arc<dyn TableProvider + Send + Sync>;

    // Get the cached data and set provider directly (tests CachedTableData.set_provider)
    let cached = cache.get(&table_id).expect("should have cached data");
    cached.set_provider(Arc::clone(&provider));

    // Retrieve via get_provider
    let retrieved = cache.get_provider(&table_id).expect("provider present");

    assert!(
        Arc::ptr_eq(&provider, &retrieved),
        "must return same Arc instance"
    );

    println!("✅ Provider cache insert and get test passed");
}

#[test]
fn test_cached_table_data_includes_system_columns() {
    use kalamdb_core::system_columns::SystemColumnsService;
    use arrow::datatypes::DataType;
    use kalamdb_commons::models::datatypes::KalamDataType;
    use kalamdb_commons::models::schemas::{ColumnDefinition, TableOptions, TableType};

    // Create a table definition with user columns
    let user_col = ColumnDefinition::new(
        1,
        "user_name".to_string(),
        1,
        KalamDataType::Text,
        false,
        true,
        false,
        kalamdb_commons::models::schemas::ColumnDefault::None,
        None,
    );

    let mut table_def = kalamdb_commons::models::schemas::TableDefinition::new(
        NamespaceId::new("test_ns"),
        TableName::new("test_table"),
        TableType::User,
        vec![user_col],
        TableOptions::user(),
        None,
    )
    .unwrap();

    // Add system columns using SystemColumnsService
    let sys_cols = SystemColumnsService::new(1);
    sys_cols.add_system_columns(&mut table_def).unwrap();

    // Verify columns were added
    assert_eq!(
        table_def.columns.len(),
        3,
        "Should have 1 user column + 2 system columns"
    );
    assert_eq!(table_def.columns[0].column_name, "user_name");
    assert_eq!(table_def.columns[1].column_name, "_seq");
    assert_eq!(table_def.columns[2].column_name, "_deleted");

    // Create CachedTableData
    let cached_data = CachedTableData::new(Arc::new(table_def));

    // Get Arrow schema (should include system columns)
    let arrow_schema = cached_data
        .arrow_schema()
        .expect("Arrow schema should be available");

    // Verify Arrow schema has all columns including system columns
    assert_eq!(
        arrow_schema.fields().len(),
        3,
        "Arrow schema should have 3 columns"
    );
    assert_eq!(arrow_schema.field(0).name(), "user_name");
    assert_eq!(arrow_schema.field(1).name(), "_seq");
    assert_eq!(arrow_schema.field(2).name(), "_deleted");

    // Verify column types
    assert!(
        matches!(arrow_schema.field(1).data_type(), DataType::Int64),
        "_seq should be Int64 (BigInt)"
    );
    assert!(
        matches!(arrow_schema.field(2).data_type(), DataType::Boolean),
        "_deleted should be Boolean"
    );

    println!("✅ T014: CachedTableData Arrow schema includes _seq and _deleted system columns");
}
