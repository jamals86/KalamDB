//! Basic cluster operations test: Create table, insert, update, delete with consistency checks.

#[path = "../common/testserver/mod.rs"]
#[allow(dead_code)]
mod test_support;

use kalam_link::models::ResponseStatus;

#[tokio::test]
async fn test_cluster_basic_crud_operations() {
    // Get the cluster
    let cluster = test_support::http_server::get_cluster_server().await;

    // Pick a random node to create table and insert data
    let namespace = format!("test_cluster_{}", std::process::id());
    let table = "messages";

    // Step 1: Create namespace on a random node
    eprintln!("\n📝 Step 1: Creating namespace on random node...");
    let response = cluster
        .execute_sql_on_random(&format!("CREATE NAMESPACE {}", namespace))
        .await
        .expect("Failed to create namespace");
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Create namespace failed: {:?}",
        response.error
    );
    eprintln!("✅ Namespace created");

    // Verify namespace exists on all nodes
    eprintln!("   ⏳ Waiting for replication...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let ns_query = format!(
        "SELECT COUNT(*) as cnt FROM system.namespaces WHERE namespace_id = '{}'",
        namespace
    );
    let consistency = cluster
        .verify_data_consistency(&ns_query)
        .await
        .expect("Failed to verify namespace consistency");
    assert!(consistency, "Namespace not replicated to all nodes");
    eprintln!("✅ Namespace replicated to all nodes");

    // Step 2: Create USER table on random node
    eprintln!("\n📝 Step 2: Creating USER table on random node...");
    let create_table_sql = format!(
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, content TEXT, status TEXT) WITH (TYPE='USER', STORAGE_ID='local')",
        namespace, table
    );
    let response = cluster
        .execute_sql_on_random(&create_table_sql)
        .await
        .expect("Failed to create table");
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Create table failed: {:?}",
        response.error
    );
    eprintln!("✅ Table created");

    // Verify table exists on all nodes
    eprintln!("   ⏳ Waiting for replication...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let table_query = format!(
        "SELECT COUNT(*) as cnt FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
        namespace, table
    );
    let consistency = cluster
        .verify_data_consistency(&table_query)
        .await
        .expect("Failed to verify table consistency");
    assert!(consistency, "Table not replicated to all nodes");
    eprintln!("✅ Table replicated to all nodes");

    // Step 3: Insert data on random node
    eprintln!("\n📝 Step 3: Inserting data on random node...");
    let insert_sql = format!(
        "INSERT INTO {}.{} (id, content, status) VALUES (1, 'message 1', 'active'), (2, 'message 2', 'active')",
        namespace, table
    );
    let response = cluster
        .execute_sql_on_random(&insert_sql)
        .await
        .expect("Failed to insert data");
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Insert failed: {:?}",
        response.error
    );
    eprintln!("✅ Data inserted");

    // Verify data exists on all nodes
    eprintln!("   ⏳ Waiting for replication...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let select_query = format!("SELECT id, content, status FROM {}.{} ORDER BY id", namespace, table);
    let consistency = cluster
        .verify_data_consistency(&select_query)
        .await
        .expect("Failed to verify data consistency after insert");
    assert!(
        consistency,
        "Inserted data not replicated to all nodes"
    );
    eprintln!("✅ Data replicated to all nodes");

    // Verify row count
    let count_query = format!("SELECT COUNT(*) as cnt FROM {}.{}", namespace, table);
    let results = cluster
        .execute_sql_on_all(&count_query)
        .await
        .expect("Failed to get count");
    for (i, result) in results.iter().enumerate() {
        let count = result.get_i64("cnt").unwrap_or(-1);
        assert_eq!(count, 2, "Node {} has wrong row count: expected 2, got {}", i, count);
    }
    eprintln!("✅ All nodes have 2 rows");

    // Step 4: Update data on random node
    eprintln!("\n📝 Step 4: Updating data on random node...");
    let update_sql = format!(
        "UPDATE {}.{} SET status = 'updated' WHERE id = 1",
        namespace, table
    );
    let response = cluster
        .execute_sql_on_random(&update_sql)
        .await
        .expect("Failed to update data");
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Update failed: {:?}",
        response.error
    );
    eprintln!("✅ Data updated");

    // Verify update replicated to all nodes
    eprintln!("   ⏳ Waiting for replication...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let consistency = cluster
        .verify_data_consistency(&select_query)
        .await
        .expect("Failed to verify data consistency after update");
    assert!(consistency, "Updated data not replicated to all nodes");
    eprintln!("✅ Updated data replicated to all nodes");

    // Verify the actual update
    let results = cluster
        .execute_sql_on_all(&format!(
            "SELECT status FROM {}.{} WHERE id = 1",
            namespace, table
        ))
        .await
        .expect("Failed to check update");
    for (i, result) in results.iter().enumerate() {
        let status = result
            .rows_as_maps()
            .first()
            .and_then(|r| r.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            status, "updated",
            "Node {} has wrong status: expected 'updated', got '{}'",
            i, status
        );
    }
    eprintln!("✅ All nodes have updated status");

    // Step 5: Delete data on random node
    eprintln!("\n📝 Step 5: Deleting data on random node...");
    let delete_sql = format!("DELETE FROM {}.{} WHERE id = 2", namespace, table);
    let response = cluster
        .execute_sql_on_random(&delete_sql)
        .await
        .expect("Failed to delete data");
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Delete failed: {:?}",
        response.error
    );
    eprintln!("✅ Data deleted");

    // Verify delete replicated to all nodes
    eprintln!("   ⏳ Waiting for replication...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let consistency = cluster
        .verify_data_consistency(&select_query)
        .await
        .expect("Failed to verify data consistency after delete");
    assert!(consistency, "Deleted data not replicated to all nodes");
    eprintln!("✅ Deletion replicated to all nodes");

    // Verify row count after delete
    let count_query = format!("SELECT COUNT(*) as cnt FROM {}.{}", namespace, table);
    let results = cluster
        .execute_sql_on_all(&count_query)
        .await
        .expect("Failed to get count after delete");
    for (i, result) in results.iter().enumerate() {
        let count = result.get_i64("cnt").unwrap_or(-1);
        assert_eq!(count, 1, "Node {} has wrong row count after delete: expected 1, got {}", i, count);
    }
    eprintln!("✅ All nodes have 1 row after deletion");

    eprintln!("\n🎉 Cluster CRUD test passed: All operations replicated consistently across nodes!\n");
}
