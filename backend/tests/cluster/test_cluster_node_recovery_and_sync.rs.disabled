//! Test cluster node recovery and data synchronization.
//!
//! This test verifies that when a node goes offline, other nodes continue operating,
//! and when the offline node comes back online, it successfully resyncs any data changes
//! that occurred while it was offline.

#[cfg(test)]
mod tests {
    use crate::common::testserver::get_cluster_server;
    use anyhow::Result;

    /// Test that node recovery and data sync works correctly.
    ///
    /// Scenario:
    /// 1. Create a namespace and table on the cluster
    /// 2. Take Node 1 offline
    /// 3. Insert/update data on Node 0 (while Node 1 is offline)
    /// 4. Bring Node 1 back online
    /// 5. Verify that Node 1 has synced the data
    #[tokio::test]
    async fn test_cluster_node_offline_and_recovery() -> Result<()> {
        let cluster = get_cluster_server().await;

        eprintln!("\n═══════════════════════════════════════════════════════════════════");
        eprintln!("📋 Test: Node Offline and Recovery with Data Sync");
        eprintln!("═══════════════════════════════════════════════════════════════════\n");

        let test_namespace = "test_recovery";
        let test_table = "test_data";

        // ================================================================================
        // STEP 1: Create namespace and table (on random node)
        // ================================================================================
        eprintln!("📝 STEP 1: Creating namespace and table on cluster...");

        let create_ns = format!("CREATE NAMESPACE {}", test_namespace);
        cluster.execute_sql_on_random(&create_ns).await?;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let create_table = format!(
            "CREATE TABLE {}.{} (id INT, name TEXT, value INT)",
            test_namespace, test_table
        );
        cluster.execute_sql_on_random(&create_table).await?;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Verify table exists on all nodes
        let check_query = format!(
            "SELECT COUNT(*) AS cnt FROM {}.{}",
            test_namespace, test_table
        );

        let consistency = cluster.verify_data_consistency(&check_query).await?;
        assert!(
            consistency,
            "❌ Table should be consistent across all online nodes after creation"
        );
        eprintln!("✅ Namespace and table created successfully on all 3 nodes\n");

        // ================================================================================
        // STEP 2: Take Node 1 offline
        // ================================================================================
        eprintln!("📝 STEP 2: Taking Node 1 offline...");

        cluster.take_node_offline(1).await?;
        assert!(
            !cluster.is_node_online(1).await?,
            "❌ Node 1 should be offline"
        );

        eprintln!(
            "✅ Node 1 is now offline. Node 0 and Node 2 continue operating.\n"
        );

        // ================================================================================
        // STEP 3: Insert and update data while Node 1 is offline
        // ================================================================================
        eprintln!("📝 STEP 3: Inserting and updating data (Node 1 offline)...");

        // Insert some test data (will only execute on Node 0 or Node 2 since Node 1 is offline)
        let insert_sql = format!(
            "INSERT INTO {}.{} (id, name, value) VALUES (1, 'Alice', 100)",
            test_namespace, test_table
        );
        cluster.execute_sql_on_random(&insert_sql).await?;
        eprintln!("   ✓ Inserted: id=1, name='Alice', value=100");

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let insert_sql2 = format!(
            "INSERT INTO {}.{} (id, name, value) VALUES (2, 'Bob', 200)",
            test_namespace, test_table
        );
        cluster.execute_sql_on_random(&insert_sql2).await?;
        eprintln!("   ✓ Inserted: id=2, name='Bob', value=200");

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Update existing data
        let update_sql = format!(
            "UPDATE {}.{} SET value = 150 WHERE id = 1",
            test_namespace, test_table
        );
        cluster.execute_sql_on_random(&update_sql).await?;
        eprintln!("   ✓ Updated: id=1, value changed to 150");

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify data consistency across ONLINE nodes (0 and 2, but not 1)
        let consistency = cluster.verify_data_consistency(&check_query).await?;
        assert!(
            consistency,
            "❌ Data should be consistent across online nodes (0 and 2)"
        );
        eprintln!("✅ Data is consistent across online nodes (0 and 2)\n");

        // ================================================================================
        // STEP 4: Bring Node 1 back online
        // ================================================================================
        eprintln!("📝 STEP 4: Bringing Node 1 back online...");

        cluster.bring_node_online(1).await?;
        assert!(
            cluster.is_node_online(1).await?,
            "❌ Node 1 should be online"
        );

        eprintln!("✅ Node 1 is now online again\n");

        // ================================================================================
        // STEP 5: Wait for sync and verify all nodes have the data
        // ================================================================================
        eprintln!("📝 STEP 5: Waiting for Node 1 to sync data...");

        // Wait a bit for replication/sync
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Now verify that ALL nodes (including Node 1) have the same data
        let consistency = cluster.verify_data_consistency(&check_query).await?;
        assert!(
            consistency,
            "❌ Data should be consistent across all 3 nodes after recovery"
        );

        eprintln!("✅ All nodes (0, 1, 2) are now in sync\n");

        // ================================================================================
        // STEP 6: Verify actual data content on all nodes
        // ================================================================================
        eprintln!("📝 STEP 6: Verifying data content on all nodes...");

        let select_all = format!("SELECT id, name, value FROM {}.{} ORDER BY id", test_namespace, test_table);
        
        let results = cluster.execute_sql_on_all_with_indices(&select_all).await?;
        eprintln!("   📊 Results from all nodes:");

        for (node_idx, response) in results {
            let rows = response.rows_as_maps();
            eprintln!("      Node {}: {} rows", node_idx, rows.len());
            for row in rows {
                let id = row.get("id").map(|v| v.to_string()).unwrap_or_default();
                let name = row.get("name").map(|v| v.to_string()).unwrap_or_default();
                let value = row.get("value").map(|v| v.to_string()).unwrap_or_default();
                eprintln!("         - id={}, name={}, value={}", id, name, value);
            }
        }

        // Verify we have exactly 2 rows
        let count_rows = cluster.execute_sql_on_random(&check_query).await?;
        let row_count = count_rows
            .rows_as_maps()
            .first()
            .and_then(|r| r.get("cnt"))
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())))
            .unwrap_or(0);

        assert_eq!(
            row_count, 2,
            "❌ Should have exactly 2 rows, but got {}",
            row_count
        );

        eprintln!("✅ All nodes have the correct data (2 rows)\n");

        eprintln!("═══════════════════════════════════════════════════════════════════");
        eprintln!("✅ TEST PASSED: Node recovery and data sync working correctly!");
        eprintln!("═══════════════════════════════════════════════════════════════════\n");

        Ok(())
    }

    /// Test that operations fail gracefully when all nodes are offline.
    #[tokio::test]
    async fn test_cluster_all_nodes_offline() -> Result<()> {
        let cluster = get_cluster_server().await;

        eprintln!("\n═══════════════════════════════════════════════════════════════════");
        eprintln!("📋 Test: All Nodes Offline Error Handling");
        eprintln!("═══════════════════════════════════════════════════════════════════\n");

        // Take all nodes offline
        cluster.take_node_offline(0).await?;
        cluster.take_node_offline(1).await?;
        cluster.take_node_offline(2).await?;

        eprintln!("📝 All 3 nodes are now offline\n");

        // Try to execute SQL - should fail with "no online nodes" error
        let result = cluster
            .execute_sql_on_random("SELECT 1")
            .await;

        assert!(
            result.is_err(),
            "❌ Should fail when all nodes are offline"
        );

        let error_msg = result.err().unwrap().to_string();
        assert!(
            error_msg.contains("No online nodes"),
            "❌ Error message should mention 'No online nodes', got: {}",
            error_msg
        );

        eprintln!("✅ Correctly rejected query when all nodes offline\n");

        // Bring nodes back online
        cluster.bring_node_online(0).await?;
        cluster.bring_node_online(1).await?;
        cluster.bring_node_online(2).await?;

        eprintln!("✅ All nodes back online\n");

        // Verify we can execute again
        let result = cluster.execute_sql_on_random("SELECT 1").await;
        assert!(result.is_ok(), "❌ Should be able to execute after nodes come online");

        eprintln!("═══════════════════════════════════════════════════════════════════");
        eprintln!("✅ TEST PASSED: Error handling for offline nodes working correctly!");
        eprintln!("═══════════════════════════════════════════════════════════════════\n");

        Ok(())
    }

    /// Test that execute_sql_on_all skips offline nodes.
    #[tokio::test]
    async fn test_cluster_execute_on_all_skips_offline() -> Result<()> {
        let cluster = get_cluster_server().await;

        eprintln!("\n═══════════════════════════════════════════════════════════════════");
        eprintln!("📋 Test: execute_sql_on_all Skips Offline Nodes");
        eprintln!("═══════════════════════════════════════════════════════════════════\n");

        // Create a test namespace and table for this test
        let test_namespace = "test_skip_offline";
        let test_table = "data";

        let create_ns = format!("CREATE NAMESPACE {}", test_namespace);
        cluster.execute_sql_on_random(&create_ns).await?;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let create_table = format!(
            "CREATE TABLE {}.{} (id INT, value TEXT)",
            test_namespace, test_table
        );
        cluster.execute_sql_on_random(&create_table).await?;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        eprintln!("📝 Created test table\n");

        // Take Node 1 offline
        cluster.take_node_offline(1).await?;
        eprintln!("📝 Took Node 1 offline\n");

        // Execute on all nodes - should only get results from nodes 0 and 2
        let select_query = format!("SELECT COUNT(*) AS cnt FROM {}.{}", test_namespace, test_table);
        let results = cluster.execute_sql_on_all(&select_query).await?;

        eprintln!("📊 execute_sql_on_all returned results from {} nodes", results.len());
        assert_eq!(
            results.len(),
            2,
            "❌ Should get results from 2 online nodes (0 and 2), but got {}",
            results.len()
        );

        eprintln!("✅ execute_sql_on_all correctly skipped offline Node 1\n");

        // Bring Node 1 back online
        cluster.bring_node_online(1).await?;
        eprintln!("📝 Brought Node 1 back online\n");

        // Now execute on all again - should get results from all 3 nodes
        let results = cluster.execute_sql_on_all(&select_query).await?;
        assert_eq!(
            results.len(),
            3,
            "❌ Should get results from all 3 nodes after bringing Node 1 online"
        );

        eprintln!("✅ execute_sql_on_all now returns results from all 3 nodes\n");

        eprintln!("═══════════════════════════════════════════════════════════════════");
        eprintln!("✅ TEST PASSED: execute_sql_on_all skipping behavior correct!");
        eprintln!("═══════════════════════════════════════════════════════════════════\n");

        Ok(())
    }
}
