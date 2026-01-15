//! Cluster consistency tests
//!
//! Tests that verify data consistency across cluster nodes

use crate::cluster_common::*;
use crate::common::*;
use std::time::Duration;

/// Test: System table counts are consistent across all cluster nodes
#[test]
fn cluster_test_system_table_consistency() {
    if !require_cluster_running() { return; }

    println!("\n=== TEST: Cluster System Table Count Consistency ===\n");

    let urls = cluster_urls();
    assert!(
        urls.len() >= 3,
        "Expected at least 3 cluster URLs, got {}",
        urls.len()
    );

    let queries = [
        ("system.tables", "SELECT count(*) as count FROM system.tables"),
        ("system.users", "SELECT count(*) as count FROM system.users"),
        ("system.namespaces", "SELECT count(*) as count FROM system.namespaces"),
    ];

    for (label, sql) in queries {
        let mut consistent = false;
        let mut last_counts = Vec::new();

        for _ in 0..10 {
            let mut counts = Vec::new();
            for url in &urls {
                let count = query_count_on_url(url, sql);
                counts.push((url.clone(), count));
            }

            let expected = counts.first().map(|(_, count)| *count).unwrap_or(0);
            let mismatch = counts.iter().any(|(_, count)| *count != expected);

            if !mismatch {
                consistent = true;
                println!("  ✓ {} count consistent across nodes: {}", label, expected);
                break;
            }

            last_counts = counts;
            std::thread::sleep(Duration::from_millis(200));
        }

        if !consistent {
            panic!("{} counts mismatch: {:?}", label, last_counts);
        }
    }

    println!("\n  ✅ System table counts consistent across cluster nodes\n");
}

/// Test: Namespace creation is replicated to all nodes
#[test]
fn cluster_test_namespace_replication() {
    if !require_cluster_running() { return; }

    println!("\n=== TEST: Namespace Replication ===\n");

    let urls = cluster_urls();
    let namespace = generate_unique_namespace("cluster_ns");

    // Create namespace on first node
    println!("Creating namespace on node 0: {}", namespace);
    execute_on_node(&urls[0], &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Small delay for replication
    std::thread::sleep(Duration::from_millis(500));

    // Verify namespace exists on all nodes
    println!("Verifying namespace exists on all nodes...");
    for (i, url) in urls.iter().enumerate() {
        let result = execute_on_node(
            url,
            &format!(
                "SELECT namespace_id FROM system.namespaces WHERE namespace_id = '{}'",
                namespace
            ),
        )
        .expect("Query failed");

        assert!(
            result.contains(&namespace),
            "Namespace not found on node {}: {}",
            i,
            result
        );
        println!("  ✓ Node {} has namespace", i);
    }

    // Cleanup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Namespace replicated to all cluster nodes\n");
}

/// Test: Table creation is replicated to all nodes
#[test]
fn cluster_test_table_replication() {
    if !require_cluster_running() { return; }

    println!("\n=== TEST: Table Replication ===\n");

    let urls = cluster_urls();
    let namespace = generate_unique_namespace("cluster_tbl");

    // Setup namespace
    let _ = execute_on_node(
        &urls[0],
        &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace),
    );
    std::thread::sleep(Duration::from_millis(200));
    execute_on_node(&urls[0], &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create tables of different types
    let tables = vec![
        (
            "users_tbl",
            format!(
                "CREATE USER TABLE {}.users_tbl (id BIGINT PRIMARY KEY, name STRING)",
                namespace
            ),
        ),
        (
            "shared_tbl",
            format!(
                "CREATE SHARED TABLE {}.shared_tbl (id BIGINT PRIMARY KEY, data STRING)",
                namespace
            ),
        ),
        (
            "stream_tbl",
            format!(
                "CREATE STREAM TABLE {}.stream_tbl (id BIGINT PRIMARY KEY, event STRING) WITH (TTL_SECONDS = 3600)",
                namespace
            ),
        ),
    ];

    // Create tables on node 0
    for (name, sql) in &tables {
        println!("Creating table on node 0: {}", name);
        execute_on_node(&urls[0], sql).expect(&format!("Failed to create {}", name));
    }

    std::thread::sleep(Duration::from_millis(500));

    // Verify tables exist on all nodes
    println!("Verifying tables exist on all nodes...");
    for (i, url) in urls.iter().enumerate() {
        for (name, _) in &tables {
            let result = execute_on_node(
                url,
                &format!(
                    "SELECT table_name FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
                    namespace, name
                ),
            )
            .expect("Query failed");

            assert!(
                result.contains(*name),
                "Table {} not found on node {}: {}",
                name,
                i,
                result
            );
        }
        println!("  ✓ Node {} has all {} tables", i, tables.len());
    }

    // Cleanup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ All table types replicated to cluster nodes\n");
}

/// Test: Data written to leader is readable from followers
#[test]
fn cluster_test_data_consistency() {
    if !require_cluster_running() { return; }

    println!("\n=== TEST: Data Consistency Across Nodes ===\n");

    let urls = cluster_urls();
    let namespace = generate_unique_namespace("cluster_data");

    // Setup
    let _ = execute_on_node(
        &urls[0],
        &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace),
    );
    std::thread::sleep(Duration::from_millis(200));
    execute_on_node(&urls[0], &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");
    std::thread::sleep(Duration::from_millis(500));

    execute_on_node(
        &urls[0],
        &format!(
            "CREATE SHARED TABLE {}.data_test (id BIGINT PRIMARY KEY, value STRING)",
            namespace
        ),
    )
    .expect("Failed to create table");

    // Wait for table to replicate to all nodes
    std::thread::sleep(Duration::from_millis(1000));
    if !wait_for_table_on_all_nodes(&namespace, "data_test", 15000) {
        panic!("Table data_test did not replicate to all nodes");
    }

    // Insert data on node 0
    println!("Inserting 100 rows on node 0...");
    let mut values = Vec::new();
    for i in 0..100 {
        values.push(format!("({}, 'value_{}')", i, i));

        if values.len() == 20 || i == 99 {
            execute_on_node(
                &urls[0],
                &format!(
                    "INSERT INTO {}.data_test (id, value) VALUES {}",
                    namespace,
                    values.join(", ")
                ),
            )
            .expect("Insert failed");
            values.clear();
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    std::thread::sleep(Duration::from_millis(1000));

    // Verify data on all nodes
    println!("Verifying data on all nodes...");
    for (i, url) in urls.iter().enumerate() {
        let mut count = 0;
        for _ in 0..10 {
            count = query_count_on_url(
                url,
                &format!("SELECT count(*) as count FROM {}.data_test", namespace),
            );
            if count == 100 {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        assert_eq!(count, 100, "Node {} has {} rows, expected 100", i, count);
        println!("  ✓ Node {} has 100 rows", i);
    }

    // Cleanup
    let _ = execute_on_node(&urls[0], &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Data consistent across all cluster nodes\n");
}
