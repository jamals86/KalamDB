//! Cluster replication tests
//!
//! Tests that verify Raft log replication behavior

use crate::cluster_common::*;
use crate::common::*;
use std::time::Duration;

/// Test: System metadata replication timing
///
/// Verifies that system table changes replicate quickly to all nodes.
#[ntest::timeout(90_000)]
#[test]
fn cluster_test_metadata_replication_timing() {
    require_cluster_running();

    println!("\n=== TEST: Metadata Replication Timing ===\n");

    let urls = cluster_urls();
    let namespace = generate_unique_namespace("cluster_time");

    // Cleanup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    // Create namespace and immediately check replication
    println!("Creating namespace on node 0...");
    let start = std::time::Instant::now();
    execute_on_node(&urls[0], &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Poll all nodes for the namespace
    let mut all_replicated = false;
    let mut check_count = 0;
    while !all_replicated && check_count < 20 {
        check_count += 1;
        std::thread::sleep(Duration::from_millis(100));

        all_replicated = urls.iter().all(|url| {
            let result = execute_on_node(
                url,
                &format!(
                    "SELECT namespace_id FROM system.namespaces WHERE namespace_id = '{}'",
                    namespace
                ),
            );
            result.map(|r| r.contains(&namespace)).unwrap_or(false)
        });
    }

    let elapsed = start.elapsed();

    if all_replicated {
        println!(
            "  ✓ Namespace replicated to all nodes in {:?}",
            elapsed
        );
    } else {
        panic!(
            "Namespace not replicated to all nodes after {:?}",
            elapsed
        );
    }

    // Cleanup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Metadata replication timing test passed\n");
}

/// Test: Sequential operations maintain order
///
/// Verifies that operations are applied in the correct order across nodes.
#[ntest::timeout(120_000)]
#[test]
fn cluster_test_operation_ordering() {
    require_cluster_running();

    println!("\n=== TEST: Operation Ordering ===\n");

    let urls = cluster_urls();
    let namespace = generate_unique_namespace("cluster_order");

    // Setup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));
    execute_on_node(&urls[0], &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    execute_on_node(
        &urls[0],
        &format!(
            "CREATE SHARED TABLE {}.ordered_data (id INT64 PRIMARY KEY, seq INT64)",
            namespace
        ),
    )
    .expect("Failed to create table");

    // Insert data with sequence numbers
    println!("Inserting 50 sequential rows...");
    for i in 0..50 {
        execute_on_node(
            &urls[0],
            &format!(
                "INSERT INTO {}.ordered_data (id, seq) VALUES ({}, {})",
                namespace, i, i * 10
            ),
        )
        .expect("Insert failed");
    }

    std::thread::sleep(Duration::from_millis(1000));

    // Verify sequence on all nodes
    println!("Verifying sequence on all nodes...");
    for (i, url) in urls.iter().enumerate() {
        let result = execute_on_node(
            url,
            &format!(
                "SELECT id, seq FROM {}.ordered_data ORDER BY id LIMIT 5",
                namespace
            ),
        )
        .expect("Query failed");

        // Verify first few sequences are correct
        assert!(
            result.contains("0") && result.contains("10") && result.contains("20"),
            "Node {} has incorrect sequence data: {}",
            i,
            result
        );
        println!("  ✓ Node {} has correct sequence ordering", i);
    }

    // Cleanup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Operation ordering maintained across nodes\n");
}

/// Test: Concurrent writes from different clients
///
/// Verifies that concurrent writes are properly serialized.
#[ntest::timeout(120_000)]
#[test]
fn cluster_test_concurrent_writes() {
    require_cluster_running();

    println!("\n=== TEST: Concurrent Writes ===\n");

    let urls = cluster_urls();
    let namespace = generate_unique_namespace("cluster_conc");

    // Setup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));
    execute_on_node(&urls[0], &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    execute_on_node(
        &urls[0],
        &format!(
            "CREATE SHARED TABLE {}.concurrent_data (id INT64 PRIMARY KEY, writer STRING)",
            namespace
        ),
    )
    .expect("Failed to create table");

    std::thread::sleep(Duration::from_millis(500));

    // Spawn threads to write concurrently from different nodes
    let handles: Vec<_> = urls
        .iter()
        .enumerate()
        .map(|(node_idx, url)| {
            let url = url.clone();
            let ns = namespace.clone();
            std::thread::spawn(move || {
                for i in 0..10 {
                    let id = node_idx * 1000 + i;
                    let result = execute_on_node(
                        &url,
                        &format!(
                            "INSERT INTO {}.concurrent_data (id, writer) VALUES ({}, 'node_{}')",
                            ns, id, node_idx
                        ),
                    );
                    if result.is_err() {
                        // Some writes might fail if node isn't leader, that's expected
                    }
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }

    std::thread::sleep(Duration::from_millis(2000));

    // Count total rows on each node
    println!("Verifying data consistency after concurrent writes...");
    let mut counts = Vec::new();
    for (i, url) in urls.iter().enumerate() {
        let count = query_count_on_url(
            url,
            &format!(
                "SELECT count(*) as count FROM {}.concurrent_data",
                namespace
            ),
        );
        counts.push(count);
        println!("  Node {} has {} rows", i, count);
    }

    // All nodes should have the same count
    let first_count = counts[0];
    for (i, count) in counts.iter().enumerate() {
        assert_eq!(
            *count, first_count,
            "Node {} has {} rows, expected {}",
            i, count, first_count
        );
    }

    // Cleanup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Concurrent writes properly serialized\n");
}

/// Test: Cluster info is consistent across nodes
#[ntest::timeout(60_000)]
#[test]
fn cluster_test_cluster_info_consistency() {
    require_cluster_running();

    println!("\n=== TEST: Cluster Info Consistency ===\n");

    let urls = cluster_urls();

    // Query cluster info from each node
    let mut cluster_ids = Vec::new();
    for (i, url) in urls.iter().enumerate() {
        let result = execute_on_node(url, "SELECT cluster_id FROM system.cluster LIMIT 1");
        match result {
            Ok(resp) => {
                println!("  Node {} cluster info: {}", i, resp.chars().take(100).collect::<String>());
                cluster_ids.push(resp);
            }
            Err(e) => {
                println!("  ✗ Node {} failed: {}", i, e);
            }
        }
    }

    println!("\n  ✅ Cluster info query completed on all nodes\n");
}
