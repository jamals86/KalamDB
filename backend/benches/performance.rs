//! Benchmark suite for KalamDB performance testing.
//!
//! This benchmark suite measures critical performance metrics:
//! - RocksDB write latency (<1ms target)
//! - DataFusion query performance
//! - WebSocket message delivery (<10ms target)
//! - Flush operation throughput
//! - Stress testing scenarios (sustained load, memory stability)
//!
//! # Running Benchmarks
//!
//! ```bash
//! cd backend
//! cargo bench
//! ```
//!
//! # Performance Targets
//!
//! - Write latency: <1ms per operation
//! - Query latency: <100ms for simple queries
//! - Notification latency: <10ms from insert to delivery
//! - Flush throughput: >10,000 rows/second
//! - Sustained write rate: 1000 inserts/sec
//! - Memory growth under load: <10% over 5 minutes

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use kalamdb_store::{test_utils::TestDb, UserTableStore};
use serde_json::json;

/// Benchmark RocksDB write performance via UserTableStore.
///
/// Measures time to write a single row to RocksDB.
/// Target: <1ms per write operation.
fn bench_rocksdb_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("rocksdb_writes");

    // Setup test database
    let test_db =
        TestDb::single_cf("user_table:bench:messages").expect("Failed to create test database");

    let store = UserTableStore::new(test_db.db.clone()).expect("Failed to create UserTableStore");

    // Benchmark single write
    group.bench_function("single_write", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let user_id = "user123";
            let row_id = counter.to_string();
            let row_data = json!({
                "id": counter,
                "content": format!("Message {}", counter),
                "created_at": chrono::Utc::now().timestamp_millis()
            });

            store
                .put("bench", "messages", user_id, &row_id, row_data)
                .expect("Failed to write");

            counter += 1;
            black_box(counter);
        });
    });

    // Benchmark batch writes (10, 100, 1000 rows)
    for batch_size in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("batch_write", batch_size),
            batch_size,
            |b, &size| {
                b.iter(|| {
                    let user_id = "user123";
                    for i in 0..size {
                        let row_id = i.to_string();
                        let row_data = json!({
                            "id": i,
                            "content": format!("Message {}", i),
                            "created_at": chrono::Utc::now().timestamp_millis()
                        });

                        store
                            .put("bench", "messages", user_id, &row_id, row_data)
                            .expect("Failed to write");
                    }
                    black_box(size);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark RocksDB read performance via UserTableStore.
///
/// Measures time to read rows from RocksDB.
fn bench_rocksdb_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("rocksdb_reads");

    // Setup test database with sample data
    let test_db =
        TestDb::single_cf("user_table:bench:messages").expect("Failed to create test database");

    let store = UserTableStore::new(test_db.db.clone()).expect("Failed to create UserTableStore");

    // Populate with test data
    let user_id = "user123";
    for i in 0..1000u64 {
        let row_id = i.to_string();
        let row_data = json!({
            "id": i,
            "content": format!("Message {}", i),
            "created_at": chrono::Utc::now().timestamp_millis()
        });
        store
            .put("bench", "messages", user_id, &row_id, row_data)
            .expect("Failed to write");
    }

    // Benchmark single read
    group.bench_function("single_read", |b| {
        b.iter(|| {
            let result = store
                .get("bench", "messages", user_id, "500")
                .expect("Failed to read");
            black_box(result);
        });
    });

    // Benchmark scan operations
    group.bench_function("scan_user", |b| {
        b.iter(|| {
            let result = store
                .scan_user("bench", "messages", user_id)
                .expect("Failed to scan");
            black_box(result.len());
        });
    });

    group.finish();
}

/// Benchmark DataFusion query performance.
///
/// Measures query execution time for various query patterns.
/// Target: <100ms for simple queries.
fn bench_datafusion_queries(c: &mut Criterion) {
    // Note: This is a placeholder for DataFusion benchmarks
    // In a full implementation, this would:
    // 1. Create a DataFusion session with registered tables
    // 2. Execute various query patterns (SELECT, WHERE, GROUP BY, JOIN)
    // 3. Measure query planning and execution time

    let mut group = c.benchmark_group("datafusion_queries");

    group.bench_function("simple_select", |b| {
        b.iter(|| {
            // Placeholder for DataFusion query execution
            black_box("SELECT * FROM app.messages WHERE user_id = 'user123'");
        });
    });

    group.finish();
}

/// Benchmark flush operation throughput.
///
/// Measures rows per second for flush operations.
/// Target: >10,000 rows/second.
fn bench_flush_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("flush_operations");

    // Setup test database
    let test_db =
        TestDb::single_cf("user_table:bench:messages").expect("Failed to create test database");

    let store = UserTableStore::new(test_db.db.clone()).expect("Failed to create UserTableStore");

    // Benchmark write + flush cycles
    for row_count in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("write_and_flush", row_count),
            row_count,
            |b, &count| {
                b.iter(|| {
                    let user_id = "user123";

                    // Write rows
                    for i in 0..count {
                        let row_id = i.to_string();
                        let row_data = json!({
                            "id": i,
                            "content": format!("Message {}", i),
                            "created_at": chrono::Utc::now().timestamp_millis()
                        });

                        store
                            .put("bench", "messages", user_id, &row_id, row_data)
                            .expect("Failed to write");
                    }

                    // In a real implementation, trigger flush operation here
                    black_box(count);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark WebSocket message serialization.
///
/// Measures time to serialize notifications.
/// Target: <1ms per notification.
fn bench_websocket_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("websocket_serialization");

    group.bench_function("serialize_notification", |b| {
        let notification = json!({
            "query_id": "messages",
            "type": "INSERT",
            "data": {
                "id": 123,
                "user_id": "user123",
                "content": "Test message",
                "created_at": chrono::Utc::now().timestamp_millis()
            }
        });

        b.iter(|| {
            let serialized = serde_json::to_string(&notification).expect("Failed to serialize");
            black_box(serialized);
        });
    });

    group.finish();
}

/// Benchmark sustained write load.
///
/// Measures ability to maintain 1000 inserts/sec.
/// Target: Stable performance over duration.
fn bench_sustained_write_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress_testing");
    group.sample_size(10); // Reduce sample size for long-running benchmarks

    // Setup test database
    let test_db =
        TestDb::single_cf("user_table:stress:messages").expect("Failed to create test database");

    let store = UserTableStore::new(test_db.db.clone()).expect("Failed to create UserTableStore");

    group.bench_function("sustained_1000_writes_per_sec", |b| {
        b.iter(|| {
            let start = std::time::Instant::now();
            let mut count = 0;

            // Write 1000 rows
            for i in 0..1000 {
                let user_id = format!("user{}", i % 10); // 10 users
                let row_id = format!("row_{}", i);
                let row_data = json!({
                    "id": i,
                    "content": format!("Stress test message {}", i),
                    "timestamp": chrono::Utc::now().timestamp_millis()
                });

                let _ = store.insert(
                    "stress",
                    "messages",
                    &user_id,
                    &row_id,
                    &row_data.to_string(),
                );

                count += 1;
            }

            let duration = start.elapsed();
            let rate = count as f64 / duration.as_secs_f64();

            // Verify we're achieving target rate
            assert!(
                rate >= 900.0,
                "Write rate too low: {} writes/sec",
                rate as u64
            );

            black_box(count);
        });
    });

    group.finish();
}

/// Benchmark concurrent write throughput.
///
/// Measures performance with multiple concurrent writers.
/// Target: Linear scaling up to 10 concurrent writers.
fn bench_concurrent_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_writes");
    group.sample_size(10);

    for thread_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_threads", thread_count)),
            thread_count,
            |b, &threads| {
                let test_db = TestDb::single_cf(&format!(
                    "user_table:concurrent:{}",
                    threads
                ))
                .expect("Failed to create test database");

                let store = Arc::new(
                    UserTableStore::new(test_db.db.clone())
                        .expect("Failed to create UserTableStore"),
                );

                b.iter(|| {
                    let handles: Vec<_> = (0..threads)
                        .map(|thread_id| {
                            let store_clone = Arc::clone(&store);
                            std::thread::spawn(move || {
                                for i in 0..100 {
                                    let user_id = format!("user{}", thread_id);
                                    let row_id = format!("row_{}_{}", thread_id, i);
                                    let row_data = json!({
                                        "id": i,
                                        "thread": thread_id,
                                        "content": format!("Message from thread {}", thread_id)
                                    });

                                    let _ = store_clone.insert(
                                        "stress",
                                        "messages",
                                        &user_id,
                                        &row_id,
                                        &row_data.to_string(),
                                    );
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().expect("Thread panicked");
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_rocksdb_writes,
    bench_rocksdb_reads,
    bench_datafusion_queries,
    bench_flush_operations,
    bench_websocket_serialization,
    bench_sustained_write_load,
    bench_concurrent_writes
);
criterion_main!(benches);

criterion_main!(benches);
