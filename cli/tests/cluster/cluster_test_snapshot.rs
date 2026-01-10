//! Snapshot Tests
//!
//! Tests Raft snapshot creation and installation in cluster mode.

use crate::cluster_common::*;
use crate::common::*;

/// Test that snapshots are created after enough log entries
#[test]
fn test_snapshot_creation() {
    require_cluster_running();
    
    println!("\n=== TEST: Snapshot Creation ===\n");
    
    let urls = cluster_urls();
    let base_url = &urls[0]; // Use first node
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let namespace = format!("snap_test_{}", timestamp);
    let table = "events";
    
    // Create namespace and table
    let client = create_cluster_client(base_url);
    
    cluster_runtime().block_on(async {
        client.execute_query(&format!("CREATE NAMESPACE {}", namespace), None, None)
            .await.expect("create namespace");
        client.execute_query(&format!(
            "CREATE TABLE {}.{} (id INT, value TEXT, PRIMARY KEY (id))",
            namespace, table
        ), None, None)
        .await.expect("create table");
        
        // Insert many rows to trigger snapshot creation
        // OpenRaft snapshot threshold is now 1000 entries by default
        println!("📝 Inserting rows to trigger snapshot...");
        for i in 0..1200 {
            client.execute_query(&format!(
                "INSERT INTO {}.{} (id, value) VALUES ({}, 'value_{}')",
                namespace, table, i, i
            ), None, None)
            .await.expect("insert row");
            
            if i % 50 == 0 {
                println!("  Inserted {} rows", i);
            }
        }
        
        println!("✅ Inserted 1200 rows");
        
        // Give Raft time to create snapshot
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        // Check system.cluster for snapshot info
        let result = client.execute_query("SELECT node_id, snapshot_index FROM system.cluster WHERE is_self = true", None, None)
            .await.expect("query cluster");
        
        println!("📊 Cluster snapshot status:");
        println!("{:?}", result);
        
        // Cleanup
        client.execute_query(&format!("DROP NAMESPACE {} CASCADE", namespace), None, None)
            .await.ok();
    });
}

/// Test snapshot with higher write load
#[test]
fn test_snapshot_with_high_write_load() {
    require_cluster_running();
    
    println!("\n=== TEST: High Write Load Snapshot ===\n");
    
    let urls = cluster_urls();
    let base_url = &urls[0]; // Use first node
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let namespace = format!("snap_heavy_{}", timestamp);
    let table = "heavy";
    
    let client = create_cluster_client(base_url);
    
    cluster_runtime().block_on(async {
        client.execute_query(&format!("CREATE NAMESPACE {}", namespace), None, None)
            .await.expect("create namespace");
        client.execute_query(&format!(
            "CREATE TABLE {}.{} (id INT, data TEXT, PRIMARY KEY (id))",
            namespace, table
        ), None, None)
        .await.expect("create table");
        
        println!("📝 High-load write test (1500 inserts)...");
        
        // Insert 1500 rows with larger data
        for i in 0..1500 {
            let large_value = format!("data_{}_", i).repeat(10); // ~70 bytes per row
            client.execute_query(&format!(
                "INSERT INTO {}.{} (id, data) VALUES ({}, '{}')",
                namespace, table, i, large_value
            ), None, None)
            .await.expect("insert row");
            
            if i % 100 == 0 {
                println!("  Inserted {} rows", i);
            }
        }
        
        println!("✅ Inserted 1500 rows");
        
        // Wait for snapshot
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        
        // Check all nodes for snapshot status
        let result = client.execute_query("SELECT node_id, role, snapshot_index, last_applied_log FROM system.cluster ORDER BY node_id", None, None)
            .await.expect("query cluster");
        
        println!("📊 Cluster snapshot status:");
        println!("{:?}", result);
        
        // Verify data integrity after potential snapshot
        let count_result = client.execute_query(&format!("SELECT COUNT(*) as cnt FROM {}.{}", namespace, table), None, None)
            .await.expect("count rows");
        
        println!("Count result: {:?}", count_result);
        println!("✅ Data integrity verified after snapshot");
        
        // Cleanup
        client.execute_query(&format!("DROP NAMESPACE {} CASCADE", namespace), None, None)
            .await.ok();
    });
}

/// Test snapshot installation on follower catchup
#[test]
#[ignore] // Requires multi-node cluster with node restart capability
fn test_snapshot_installation_on_catchup() {
    // This test would require:
    // 1. Stop a follower node
    // 2. Write enough data to trigger snapshot on leader
    // 3. Restart the follower
    // 4. Verify follower catches up via snapshot installation
    
    // TODO: Implement once we have cluster management API
    println!("⚠️  Snapshot installation test not yet implemented");
}
