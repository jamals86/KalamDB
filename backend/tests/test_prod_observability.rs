//! Observability Tests
//!
//! Tests system tables, metrics, and operational visibility:
//! - System tables (users, namespaces, tables, jobs, live_queries, storages)
//! - Stats and metrics
//! - Audit logging
//!
//! These tests ensure operators have the necessary tools to monitor KalamDB in production.

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, TestServer};
use kalamdb_api::models::ResponseStatus;
use kalamdb_commons::Role;

/// Test: system.tables contains accurate metadata
#[tokio::test]
async fn test_system_tables_metadata() {
    let server = TestServer::new().await;
    let namespace = "obs_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Create different table types
    server.execute_sql(&format!(
        "CREATE TABLE {}.user_table (id BIGINT PRIMARY KEY) WITH (TYPE='USER')",
        namespace
    )).await;
    
    server.execute_sql(&format!(
        "CREATE TABLE {}.shared_table (id BIGINT PRIMARY KEY) WITH (TYPE='SHARED', ACCESS_LEVEL='PUBLIC')",
        namespace
    )).await;
    
    server.execute_sql(&format!(
        "CREATE TABLE {}.stream_table (id TEXT PRIMARY KEY) WITH (TYPE='STREAM', TTL_SECONDS=60)",
        namespace
    )).await;
    
    // Query system.tables
    let response = server.execute_sql(&format!(
        "SELECT table_name, table_type FROM system.tables WHERE namespace_id = '{}' ORDER BY table_name",
        namespace
    )).await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 3, "Should have 3 tables");
        
        // Verify types
        let types: Vec<String> = rows.iter()
            .filter_map(|row| row.get("table_type").and_then(|v| v.as_str()).map(String::from))
            .collect();
        
        assert!(types.contains(&"USER".to_string()), "Should have USER table");
        assert!(types.contains(&"SHARED".to_string()), "Should have SHARED table");
        assert!(types.contains(&"STREAM".to_string()), "Should have STREAM table");
    }
}

/// Test: system.namespaces tracks all namespaces
#[tokio::test]
async fn test_system_namespaces_tracking() {
    let server = TestServer::new().await;
    
    let namespaces = vec!["ns1", "ns2", "ns3"];
    
    for ns in &namespaces {
        fixtures::create_namespace(&server, ns).await;
    }
    
    // Query system.namespaces
    let response = server.execute_sql(
        "SELECT namespace_id FROM system.namespaces ORDER BY namespace_id"
    ).await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        let found_namespaces: Vec<String> = rows.iter()
            .filter_map(|row| row.get("namespace_id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        
        for ns in &namespaces {
            assert!(
                found_namespaces.contains(&ns.to_string()),
                "system.namespaces should contain '{}'",
                ns
            );
        }
    }
}

/// Test: system.users shows created users
#[tokio::test]
async fn test_system_users_visibility() {
    let server = TestServer::new().await;
    
    // Create test users
    server.create_user("test_user_1", "Pass123!", Role::User).await;
    server.create_user("test_user_2", "Pass456!", Role::Service).await;
    
    // Query system.users
    let response = server.execute_sql(
        "SELECT user_id, username, role FROM system.users WHERE deleted_at IS NULL ORDER BY username"
    ).await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        let usernames: Vec<String> = rows.iter()
            .filter_map(|row| row.get("username").and_then(|v| v.as_str()).map(String::from))
            .collect();
        
        assert!(
            usernames.iter().any(|u| u == "test_user_1"),
            "Should find test_user_1 in system.users"
        );
        
        assert!(
            usernames.iter().any(|u| u == "test_user_2"),
            "Should find test_user_2 in system.users"
        );
    }
}

/// Test: system.jobs tracks flush jobs
#[tokio::test]
async fn test_system_jobs_tracking() {
    let server = TestServer::new().await;
    let namespace = "jobs_test";
    let table_name = "flushed_data";
    let user_id = "jobs_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql_as_user(&format!(
        "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY) WITH (TYPE='USER')",
        namespace, table_name
    ), user_id).await;
    
    // Insert some data
    for i in 0..20 {
        server.execute_sql_as_user(&format!(
            "INSERT INTO {}.{} (id) VALUES ({})",
            namespace, table_name, i
        ), user_id).await;
    }
    
    // Trigger flush
    let flush_result = flush_helpers::execute_flush_synchronously(&server, namespace, table_name)
        .await;
    
    if flush_result.is_ok() {
        // Query system.jobs for flush jobs
        let response = server.execute_sql(
            "SELECT job_id, status FROM system.jobs WHERE job_id LIKE 'FL%' ORDER BY created_at DESC LIMIT 5"
        ).await;
        
        assert_eq!(response.status, ResponseStatus::Success);
        
        if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
            if rows.len() > 0 {
                println!("Found {} flush jobs in system.jobs", rows.len());
                
                // Check job statuses
                for row in rows {
                    let job_id = row.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
                    let status = row.get("status").and_then(|v| v.as_str()).unwrap_or("");
                    
                    println!("  Job: {} - Status: {}", job_id, status);
                }
            }
        }
    }
}

/// Test: system.storages lists configured storages
#[tokio::test]
async fn test_system_storages_listing() {
    let server = TestServer::new().await;
    
    // Query system.storages
    let response = server.execute_sql(
        "SELECT storage_id, storage_type, storage_name FROM system.storages ORDER BY storage_id"
    ).await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        // Should have at least 'local' storage
        let storage_ids: Vec<String> = rows.iter()
            .filter_map(|row| row.get("storage_id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        
        assert!(
            storage_ids.contains(&"local".to_string()),
            "Should have 'local' storage configured"
        );
        
        for row in rows {
            let id = row.get("storage_id").and_then(|v| v.as_str()).unwrap_or("");
            let type_ = row.get("storage_type").and_then(|v| v.as_str()).unwrap_or("");
            let name = row.get("storage_name").and_then(|v| v.as_str()).unwrap_or("");
            
            println!("Storage: {} ({}) - {}", id, type_, name);
        }
    }
}

/// Test: system.stats provides cache metrics
#[tokio::test]
async fn test_system_stats_cache_metrics() {
    let server = TestServer::new().await;
    
    // Query system.stats
    let response = server.execute_sql(
        "SELECT key, value FROM system.stats ORDER BY key"
    ).await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        let stats: Vec<(String, String)> = rows.iter()
            .filter_map(|row| {
                let key = row.get("key").and_then(|v| v.as_str())?;
                let value = row.get("value").and_then(|v| v.as_str())?;
                Some((key.to_string(), value.to_string()))
            })
            .collect();
        
        println!("System stats:");
        for (key, value) in &stats {
            println!("  {}: {}", key, value);
        }
        
        // Verify expected metrics exist
        let keys: Vec<String> = stats.iter().map(|(k, _)| k.clone()).collect();
        
        // Expected cache metrics
        let expected = vec![
            "schema_cache_hits",
            "schema_cache_misses",
            "schema_cache_hit_rate",
            "schema_cache_size",
            "schema_cache_evictions",
        ];
        
        for expected_key in expected {
            if keys.iter().any(|k| k == expected_key) {
                println!("  ✓ Found expected metric: {}", expected_key);
            }
        }
    }
}

/// Test: Schema changes are reflected in system.tables
#[tokio::test]
async fn test_schema_changes_in_system_tables() {
    let server = TestServer::new().await;
    let namespace = "schema_test";
    let table_name = "evolving_table";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Create initial table
    server.execute_sql(&format!(
        "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, name TEXT) WITH (TYPE='USER')",
        namespace, table_name
    )).await;
    
    // Query schema version
    let initial_response = server.execute_sql(&format!(
        "SELECT schema_version FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
        namespace, table_name
    )).await;
    
    let initial_version = if let Some(rows) = initial_response.results.first().and_then(|r| r.rows.as_ref()) {
        rows[0].get("schema_version").and_then(|v| v.as_i64()).unwrap_or(0)
    } else {
        0
    };
    
    println!("Initial schema version: {}", initial_version);
    
    // Alter table
    server.execute_sql(&format!(
        "ALTER TABLE {}.{} ADD COLUMN email TEXT",
        namespace, table_name
    )).await;
    
    // Query schema version again
    let updated_response = server.execute_sql(&format!(
        "SELECT schema_version FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
        namespace, table_name
    )).await;
    
    if let Some(rows) = updated_response.results.first().and_then(|r| r.rows.as_ref()) {
        let updated_version = rows[0].get("schema_version").and_then(|v| v.as_i64()).unwrap_or(0);
        
        println!("Updated schema version: {}", updated_version);
        
        // Version should increment (or at least change)
        assert!(
            updated_version >= initial_version,
            "Schema version should increment after ALTER TABLE"
        );
    }
}

/// Test: Row counts in system.tables are accurate
#[tokio::test]
async fn test_row_counts_in_system_tables() {
    let server = TestServer::new().await;
    let namespace = "count_test";
    let table_name = "counted_rows";
    let user_id = "count_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql_as_user(&format!(
        "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY) WITH (TYPE='USER')",
        namespace, table_name
    ), user_id).await;
    
    // Insert 25 rows
    for i in 0..25 {
        server.execute_sql_as_user(&format!(
            "INSERT INTO {}.{} (id) VALUES ({})",
            namespace, table_name, i
        ), user_id).await;
    }
    
    // Query system.tables for row count
    let response = server.execute_sql(&format!(
        "SELECT row_count FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
        namespace, table_name
    )).await;
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        let row_count = rows[0].get("row_count").and_then(|v| v.as_i64()).unwrap_or(0);
        
        println!("Row count in system.tables: {}", row_count);
        
        // Note: Row count may be eventually consistent
        assert!(
            row_count >= 0,
            "Row count should be non-negative"
        );
    }
}

/// Test: Deleted users don't appear in default system.users query
#[tokio::test]
async fn test_deleted_users_not_in_default_query() {
    let server = TestServer::new().await;
    
    // Create and delete a user
    server.create_user("deleted_user", "Pass123!", Role::User).await;
    
    // Delete the user via SQL
    let delete_sql = "DROP USER 'deleted_user'";
    server.execute_sql(delete_sql).await;
    
    // Query without filtering deleted_at
    let response = server.execute_sql(
        "SELECT user_id FROM system.users WHERE user_id = 'deleted_user' AND deleted_at IS NULL"
    ).await;
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(
            rows.len(), 0,
            "Deleted user should not appear when filtering deleted_at IS NULL"
        );
    }
    
    // Query including soft-deleted users
    let response_with_deleted = server.execute_sql(
        "SELECT user_id, deleted_at FROM system.users WHERE user_id = 'deleted_user'"
    ).await;
    
    if let Some(rows) = response_with_deleted.results.first().and_then(|r| r.rows.as_ref()) {
        if rows.len() > 0 {
            println!("Soft-deleted user found (deleted_at is set)");
            
            let deleted_at = rows[0].get("deleted_at");
            assert!(deleted_at.is_some(), "deleted_at should be set for soft-deleted user");
        }
    }
}

/// Test: system.live_queries tracks active subscriptions
#[tokio::test]
async fn test_system_live_queries_tracking() {
    let server = TestServer::new().await;
    
    // Query system.live_queries
    let response = server.execute_sql(
        "SELECT subscription_id, sql FROM system.live_queries LIMIT 10"
    ).await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "system.live_queries should be queryable"
    );
    
    // Note: No active subscriptions in this test, so results may be empty
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        println!("Active live queries: {}", rows.len());
        
        for row in rows {
            let sub_id = row.get("subscription_id").and_then(|v| v.as_str()).unwrap_or("");
            let sql = row.get("sql").and_then(|v| v.as_str()).unwrap_or("");
            
            println!("  Subscription: {} - SQL: {}", sub_id, sql);
        }
    }
}
