//! Integration tests for Storage Management functionality
//!
//! Tests comprehensive storage management operations:
//! 1. Default storage creation on startup
//! 2. CREATE STORAGE with various backends (filesystem, S3)
//! 3. ALTER STORAGE to update configuration
//! 4. DROP STORAGE with referential integrity checks
//! 5. SHOW STORAGES to list all configured storages
//! 6. Template validation (path_template, shared_tables_template, user_tables_template)
//! 7. Storage lookup chain (default storage, namespace storage, table storage)
//! 8. Error handling (duplicate storage_id, invalid templates, deleting in-use storage)
//!
//! Uses the REST API `/v1/api/sql` endpoint to test end-to-end functionality.

mod common;

use common::{fixtures, TestServer};

// ============================================================================
// Test 1: Default Storage Creation
// ============================================================================

#[actix_web::test]
async fn test_01_default_storage_exists() {
    let server = TestServer::new().await;

    // Use SHOW STORAGES instead of querying system.storages directly
    let response = server.execute_sql("SHOW STORAGES").await;

    assert_eq!(
        response.status, "success",
        "Failed to execute SHOW STORAGES: {:?}",
        response.error
    );

    // Verify 'local' storage exists
    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(rows.len() >= 1, "Expected at least 1 storage");

        let local_storage = &rows[0];
        assert_eq!(
            local_storage.get("storage_id").and_then(|v| v.as_str()),
            Some("local"),
            "First storage should be 'local'"
        );
        assert_eq!(
            local_storage.get("storage_type").and_then(|v| v.as_str()),
            Some("filesystem"),
            "storage_type should be 'filesystem'"
        );
        assert_eq!(
            local_storage.get("storage_name").and_then(|v| v.as_str()),
            Some("Local Filesystem"),
            "storage_name should be 'Local Filesystem'"
        );
    } else {
        panic!("No rows returned for 'local' storage");
    }
}

// ============================================================================
// Test 2: SHOW STORAGES
// ============================================================================

#[actix_web::test]
async fn test_02_show_storages_basic() {
    let server = TestServer::new().await;

    // Execute SHOW STORAGES
    let response = server.execute_sql("SHOW STORAGES").await;

    assert_eq!(
        response.status, "success",
        "SHOW STORAGES failed: {:?}",
        response.error
    );

    // Verify at least 'local' storage exists
    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(rows.len() >= 1, "Expected at least 1 storage (local)");

        // First storage should be 'local' (ordered by storage_id)
        let first_storage = &rows[0];
        assert_eq!(
            first_storage.get("storage_id").and_then(|v| v.as_str()),
            Some("local"),
            "First storage should be 'local'"
        );
    } else {
        panic!("No rows returned from SHOW STORAGES");
    }
}

// ============================================================================
// Test 3: CREATE STORAGE - Filesystem
// ============================================================================

#[actix_web::test]
async fn test_03_create_storage_filesystem() {
    let server = TestServer::new().await;

    // Create a filesystem storage
    let sql = r#"
        CREATE STORAGE archive
        TYPE filesystem
        NAME 'Archive Storage'
        DESCRIPTION 'Cold storage for archived data'
        PATH '/data/archive'
        SHARED_TABLES_TEMPLATE '/data/archive/shared/{namespace}/{tableName}'
        USER_TABLES_TEMPLATE '/data/archive/users/{namespace}/{tableName}/{userId}'
    "#;

    let response = server.execute_sql(sql).await;

    assert_eq!(
        response.status, "success",
        "CREATE STORAGE failed: {:?}",
        response.error
    );

    // Verify storage was created
    let response = server
        .execute_sql("SELECT * FROM system.storages WHERE storage_id = 'archive'")
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to query system.storages: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected exactly 1 'archive' storage");

        let archive = &rows[0];
        assert_eq!(
            archive.get("storage_id").and_then(|v| v.as_str()),
            Some("archive")
        );
        assert_eq!(
            archive.get("storage_type").and_then(|v| v.as_str()),
            Some("filesystem")
        );
        assert_eq!(
            archive.get("storage_name").and_then(|v| v.as_str()),
            Some("Archive Storage")
        );
        assert_eq!(
            archive.get("description").and_then(|v| v.as_str()),
            Some("Cold storage for archived data")
        );
    } else {
        panic!("No rows returned for 'archive' storage");
    }
}

// ============================================================================
// Test 4: CREATE STORAGE - S3
// ============================================================================

#[actix_web::test]
async fn test_04_create_storage_s3() {
    let server = TestServer::new().await;

    // Create an S3 storage
    let sql = r#"
        CREATE STORAGE s3_main
        TYPE s3
        NAME 'S3 Main Storage'
        DESCRIPTION 'Primary S3 storage bucket'
        BUCKET 'kalamdb-main'
        REGION 'us-west-2'
        SHARED_TABLES_TEMPLATE 's3://kalamdb-main/shared/{namespace}/{tableName}'
        USER_TABLES_TEMPLATE 's3://kalamdb-main/users/{namespace}/{tableName}/{userId}'
    "#;

    let response = server.execute_sql(sql).await;

    assert_eq!(
        response.status, "success",
        "CREATE STORAGE (S3) failed: {:?}",
        response.error
    );

    // Verify storage was created
    let response = server
        .execute_sql("SELECT * FROM system.storages WHERE storage_id = 's3_main'")
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to query system.storages: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected exactly 1 's3_main' storage");

        let s3_storage = &rows[0];
        assert_eq!(
            s3_storage.get("storage_id").and_then(|v| v.as_str()),
            Some("s3_main")
        );
        assert_eq!(
            s3_storage.get("storage_type").and_then(|v| v.as_str()),
            Some("s3")
        );
    } else {
        panic!("No rows returned for 's3_main' storage");
    }
}

// ============================================================================
// Test 5: CREATE STORAGE - Duplicate storage_id (Error)
// ============================================================================

#[actix_web::test]
async fn test_05_create_storage_duplicate_error() {
    let server = TestServer::new().await;

    // Try to create a storage with duplicate storage_id 'local'
    let sql = r#"
        CREATE STORAGE local
        TYPE filesystem
        NAME 'Duplicate Local'
        PATH '/data/duplicate'
    "#;

    let response = server.execute_sql(sql).await;

    assert_eq!(
        response.status, "error",
        "CREATE STORAGE should fail with duplicate storage_id"
    );

    // Verify error message mentions duplicate
    if let Some(error) = &response.error {
        assert!(
            error.message.contains("already exists") || error.message.contains("duplicate"),
            "Error should mention duplicate storage_id: {}",
            error.message
        );
    } else {
        panic!("Expected error message for duplicate storage_id");
    }
}

// ============================================================================
// Test 6: CREATE STORAGE - Invalid Template (Error)
// ============================================================================

#[actix_web::test]
async fn test_06_create_storage_invalid_template() {
    let server = TestServer::new().await;

    // Try to create storage with invalid template ordering (userId before namespace)
    let sql = r#"
        CREATE STORAGE bad_template
        TYPE filesystem
        NAME 'Bad Template Storage'
        PATH '/data/bad'
        USER_TABLES_TEMPLATE '/data/{userId}/{tableName}/{namespace}'
    "#;

    let response = server.execute_sql(sql).await;

    assert_eq!(
        response.status, "error",
        "CREATE STORAGE should fail with invalid template"
    );

    // Verify error message mentions template validation
    if let Some(error) = &response.error {
        assert!(
            error.message.contains("template")
                || error.message.contains("order")
                || error.message.contains("namespace"),
            "Error should mention template validation issue: {}",
            error.message
        );
    } else {
        panic!("Expected error message for invalid template");
    }
}

// ============================================================================
// Test 7: ALTER STORAGE - Update All Fields
// ============================================================================

#[actix_web::test]
async fn test_07_alter_storage_all_fields() {
    let server = TestServer::new().await;

    // First create a storage
    let create_sql = r#"
        CREATE STORAGE temp_storage
        TYPE filesystem
        NAME 'Temporary Storage'
        PATH '/data/temp'
    "#;

    let response = server.execute_sql(create_sql).await;
    assert_eq!(
        response.status, "success",
        "CREATE STORAGE failed: {:?}",
        response.error
    );

    // Now alter all fields
    let alter_sql = r#"
        ALTER STORAGE temp_storage
        SET NAME = 'Updated Storage'
        SET DESCRIPTION = 'Updated description'
        SET SHARED_TABLES_TEMPLATE = '/data/temp/shared/{namespace}/{tableName}'
        SET USER_TABLES_TEMPLATE = '/data/temp/users/{namespace}/{tableName}/{userId}'
    "#;

    let response = server.execute_sql(alter_sql).await;
    assert_eq!(
        response.status, "success",
        "ALTER STORAGE failed: {:?}",
        response.error
    );

    // Verify changes
    let response = server
        .execute_sql("SELECT * FROM system.storages WHERE storage_id = 'temp_storage'")
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to query: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected exactly 1 storage");

        let storage = &rows[0];
        assert_eq!(
            storage.get("storage_name").and_then(|v| v.as_str()),
            Some("Updated Storage")
        );
        assert_eq!(
            storage.get("description").and_then(|v| v.as_str()),
            Some("Updated description")
        );
        assert_eq!(
            storage.get("user_tables_template").and_then(|v| v.as_str()),
            Some("/data/temp/users/{namespace}/{tableName}/{userId}")
        );
    } else {
        panic!("No rows returned for temp_storage");
    }
}

// ============================================================================
// Test 8: ALTER STORAGE - Partial Update
// ============================================================================

#[actix_web::test]
async fn test_08_alter_storage_partial() {
    let server = TestServer::new().await;

    // Create a storage
    let create_sql = r#"
        CREATE STORAGE partial_test
        TYPE filesystem
        NAME 'Original Name'
        DESCRIPTION 'Original description'
        PATH '/data/partial'
    "#;

    let response = server.execute_sql(create_sql).await;
    assert_eq!(
        response.status, "success",
        "CREATE STORAGE failed: {:?}",
        response.error
    );

    // Only update description
    let alter_sql = "ALTER STORAGE partial_test SET DESCRIPTION = 'Updated description only'";

    let response = server.execute_sql(alter_sql).await;
    assert_eq!(
        response.status, "success",
        "ALTER STORAGE failed: {:?}",
        response.error
    );

    // Verify only description changed
    let response = server
        .execute_sql("SELECT * FROM system.storages WHERE storage_id = 'partial_test'")
        .await;

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        let storage = &rows[0];
        assert_eq!(
            storage.get("storage_name").and_then(|v| v.as_str()),
            Some("Original Name"),
            "storage_name should remain unchanged"
        );
        assert_eq!(
            storage.get("description").and_then(|v| v.as_str()),
            Some("Updated description only"),
            "description should be updated"
        );
    }
}

// ============================================================================
// Test 9: ALTER STORAGE - Invalid Template (Error)
// ============================================================================

#[actix_web::test]
async fn test_09_alter_storage_invalid_template() {
    let server = TestServer::new().await;

    // Create a storage
    let create_sql = r#"
        CREATE STORAGE alter_invalid
        TYPE filesystem
        NAME 'Alter Invalid Test'
        PATH '/data/alter_invalid'
    "#;

    let response = server.execute_sql(create_sql).await;
    assert_eq!(
        response.status, "success",
        "CREATE STORAGE failed: {:?}",
        response.error
    );

    // Try to alter with invalid template
    let alter_sql = r#"
        ALTER STORAGE alter_invalid
        SET USER_TABLES_TEMPLATE = '/data/{tableName}/{userId}/{namespace}'
    "#;

    let response = server.execute_sql(alter_sql).await;
    assert_eq!(
        response.status, "error",
        "ALTER STORAGE should fail with invalid template"
    );

    if let Some(error) = &response.error {
        assert!(
            error.message.contains("template") || error.message.contains("order"),
            "Error should mention template validation: {}",
            error.message
        );
    }
}

// ============================================================================
// Test 10: DROP STORAGE - Basic
// ============================================================================

#[actix_web::test]
async fn test_10_drop_storage_basic() {
    let server = TestServer::new().await;

    // Create a storage to drop
    let create_sql = r#"
        CREATE STORAGE drop_test
        TYPE filesystem
        NAME 'Drop Test Storage'
        PATH '/data/drop_test'
    "#;

    let response = server.execute_sql(create_sql).await;
    assert_eq!(
        response.status, "success",
        "CREATE STORAGE failed: {:?}",
        response.error
    );

    // Drop the storage
    let drop_sql = "DROP STORAGE drop_test";
    let response = server.execute_sql(drop_sql).await;
    assert_eq!(
        response.status, "success",
        "DROP STORAGE failed: {:?}",
        response.error
    );

    // Verify storage no longer exists
    let response = server
        .execute_sql("SELECT * FROM system.storages WHERE storage_id = 'drop_test'")
        .await;

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 0, "Storage should be deleted");
    }
}

// ============================================================================
// Test 11: DROP STORAGE - Referential Integrity (Error)
// ============================================================================

#[actix_web::test]
async fn test_11_drop_storage_referential_integrity() {
    let server = TestServer::new().await;

    // Create namespace and table using 'local' storage
    fixtures::create_namespace(&server, "drop_ns").await;
    fixtures::create_messages_table(&server, "drop_ns", Some("user1")).await;

    // Try to drop 'local' storage (should fail because tables reference it)
    let drop_sql = "DROP STORAGE local";
    let response = server.execute_sql(drop_sql).await;

    assert_eq!(
        response.status, "error",
        "DROP STORAGE should fail when tables reference it"
    );

    if let Some(error) = &response.error {
        assert!(
            error.message.contains("in use")
                || error.message.contains("reference")
                || error.message.contains("table"),
            "Error should mention storage is in use: {}",
            error.message
        );
    }
}

// ============================================================================
// Test 12: DROP STORAGE - Non-existent (Error)
// ============================================================================

#[actix_web::test]
async fn test_12_drop_storage_not_exists() {
    let server = TestServer::new().await;

    // Try to drop non-existent storage
    let drop_sql = "DROP STORAGE nonexistent";
    let response = server.execute_sql(drop_sql).await;

    assert_eq!(
        response.status, "error",
        "DROP STORAGE should fail for non-existent storage"
    );

    if let Some(error) = &response.error {
        assert!(
            error.message.contains("not found") || error.message.contains("does not exist"),
            "Error should mention storage not found: {}",
            error.message
        );
    }
}

// ============================================================================
// Test 13: Template Validation - Correct Ordering
// ============================================================================

#[actix_web::test]
async fn test_13_template_validation_correct_order() {
    let server = TestServer::new().await;

    // Valid template: {namespace} -> {tableName} -> {shard} -> {userId}
    let sql = r#"
        CREATE STORAGE valid_order
        TYPE filesystem
        NAME 'Valid Order Storage'
        PATH '/data/valid'
        USER_TABLES_TEMPLATE '/data/{namespace}/{tableName}/{shard}/{userId}/data'
    "#;

    let response = server.execute_sql(sql).await;
    assert_eq!(
        response.status, "success",
        "Valid template should succeed: {:?}",
        response.error
    );
}

// ============================================================================
// Test 14: Template Validation - Invalid Ordering
// ============================================================================

#[actix_web::test]
async fn test_14_template_validation_invalid_order() {
    let server = TestServer::new().await;

    // Invalid: {userId} before {namespace}
    let sql = r#"
        CREATE STORAGE invalid_order
        TYPE filesystem
        NAME 'Invalid Order Storage'
        PATH '/data/invalid'
        USER_TABLES_TEMPLATE '/data/{userId}/{namespace}/{tableName}'
    "#;

    let response = server.execute_sql(sql).await;
    assert_eq!(response.status, "error", "Invalid template should fail");
}

// ============================================================================
// Test 15: Storage Lookup Chain - Table-level Storage
// ============================================================================

#[actix_web::test]
async fn test_15_storage_lookup_table_level() {
    let server = TestServer::new().await;

    // Create custom storage
    let create_storage = r#"
        CREATE STORAGE table_storage
        TYPE filesystem
        NAME 'Table-level Storage'
        PATH '/data/table_storage'
    "#;
    server.execute_sql(create_storage).await;

    // Create namespace
    fixtures::create_namespace(&server, "lookup_ns").await;

    // Create table with explicit storage
    let create_table = r#"
        CREATE USER TABLE lookup_ns.lookup_table (
            id BIGINT,
            message TEXT
        )
        OWNER_ID 'user1'
        STORAGE 'table_storage'
        USE_USER_STORAGE
    "#;

    let response = server.execute_sql(create_table).await;
    assert_eq!(
        response.status, "success",
        "CREATE TABLE with storage failed: {:?}",
        response.error
    );

    // Verify table has correct storage
    let query =
        "SELECT * FROM system.tables WHERE namespace = 'lookup_ns' AND table_name = 'lookup_table'";
    let response = server.execute_sql(query).await;

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected 1 table");
        let table = &rows[0];
        assert_eq!(
            table.get("storage_id").and_then(|v| v.as_str()),
            Some("table_storage"),
            "Table should use 'table_storage' storage"
        );
        assert_eq!(
            table.get("use_user_storage").and_then(|v| v.as_bool()),
            Some(true),
            "Table should have use_user_storage enabled"
        );
    }
}

// ============================================================================
// Test 16: SHOW STORAGES - Multiple Storages Ordered
// ============================================================================

#[actix_web::test]
async fn test_16_show_storages_ordered() {
    let server = TestServer::new().await;

    // Create multiple storages
    server
        .execute_sql("CREATE STORAGE z_last TYPE filesystem NAME 'Z Last' PATH '/z'")
        .await;
    server
        .execute_sql("CREATE STORAGE a_first TYPE filesystem NAME 'A First' PATH '/a'")
        .await;
    server
        .execute_sql("CREATE STORAGE m_middle TYPE filesystem NAME 'M Middle' PATH '/m'")
        .await;

    // Show storages (should be ordered with 'local' first, then alphabetically)
    let response = server.execute_sql("SHOW STORAGES").await;
    assert_eq!(
        response.status, "success",
        "SHOW STORAGES failed: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(rows.len() >= 4, "Expected at least 4 storages");

        // First should be 'local'
        assert_eq!(
            rows[0].get("storage_id").and_then(|v| v.as_str()),
            Some("local"),
            "First storage should be 'local'"
        );

        // Rest should be alphabetical
        let storage_ids: Vec<&str> = rows[1..]
            .iter()
            .filter_map(|r| r.get("storage_id").and_then(|v| v.as_str()))
            .collect();

        // Check that a_first comes before m_middle and m_middle before z_last
        let a_pos = storage_ids.iter().position(|&id| id == "a_first");
        let m_pos = storage_ids.iter().position(|&id| id == "m_middle");
        let z_pos = storage_ids.iter().position(|&id| id == "z_last");

        assert!(
            a_pos.is_some() && m_pos.is_some() && z_pos.is_some(),
            "All storages should be present"
        );
        assert!(
            a_pos < m_pos && m_pos < z_pos,
            "Storages should be in alphabetical order"
        );
    }
}

// ============================================================================
// Test 17: Concurrent Storage Operations
// ============================================================================

#[actix_web::test]
async fn test_17_concurrent_storage_operations() {
    let server = TestServer::new().await;

    // Create storage
    let create =
        "CREATE STORAGE concurrent TYPE filesystem NAME 'Concurrent' PATH '/data/concurrent'";
    server.execute_sql(create).await;

    // Concurrent ALTER operations (simulated sequentially - actual concurrency would need tokio::spawn)
    let alter1 = "ALTER STORAGE concurrent SET DESCRIPTION = 'Update 1'";
    let alter2 = "ALTER STORAGE concurrent SET NAME = 'Updated Name'";

    let response1 = server.execute_sql(alter1).await;
    let response2 = server.execute_sql(alter2).await;

    assert_eq!(
        response1.status, "success",
        "First ALTER failed: {:?}",
        response1.error
    );
    assert_eq!(
        response2.status, "success",
        "Second ALTER failed: {:?}",
        response2.error
    );

    // Verify final state
    let response = server
        .execute_sql("SELECT * FROM system.storages WHERE storage_id = 'concurrent'")
        .await;

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        let storage = &rows[0];
        assert_eq!(
            storage.get("storage_name").and_then(|v| v.as_str()),
            Some("Updated Name")
        );
        assert_eq!(
            storage.get("description").and_then(|v| v.as_str()),
            Some("Update 1")
        );
    }
}

// ============================================================================
// Test 18: Storage Type Validation
// ============================================================================

#[actix_web::test]
async fn test_18_invalid_storage_type() {
    let server = TestServer::new().await;

    // Try to create storage with invalid type
    let sql = r#"
        CREATE STORAGE invalid_type_test
        TYPE invalid_backend
        NAME 'Invalid Type'
        PATH '/data/invalid'
    "#;

    let response = server.execute_sql(sql).await;
    assert_eq!(
        response.status, "error",
        "CREATE STORAGE should fail with invalid type"
    );

    if let Some(error) = &response.error {
        assert!(
            error.message.contains("storage_type")
                || error.message.contains("invalid")
                || error.message.contains("type"),
            "Error should mention invalid storage type: {}",
            error.message
        );
    }
}

// ============================================================================
// Test 19: Empty Storage Configuration
// ============================================================================

#[actix_web::test]
async fn test_19_minimal_storage_config() {
    let server = TestServer::new().await;

    // Create storage with minimal config (only required fields)
    let sql = r#"
        CREATE STORAGE minimal
        TYPE filesystem
        NAME 'Minimal Storage'
        PATH '/data/minimal'
    "#;

    let response = server.execute_sql(sql).await;
    assert_eq!(
        response.status, "success",
        "Minimal storage config should succeed: {:?}",
        response.error
    );

    // Verify defaults are applied
    let response = server
        .execute_sql("SELECT * FROM system.storages WHERE storage_id = 'minimal'")
        .await;

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        let storage = &rows[0];
        assert_eq!(
            storage.get("storage_id").and_then(|v| v.as_str()),
            Some("minimal")
        );
        assert_eq!(
            storage.get("storage_type").and_then(|v| v.as_str()),
            Some("filesystem")
        );
        // Description should be null/empty if not provided
    }
}

// ============================================================================
// Test 20: Storage Integration with Namespace
// ============================================================================

#[actix_web::test]
async fn test_20_storage_with_namespace() {
    let server = TestServer::new().await;

    // Create custom storage
    let create_storage = r#"
        CREATE STORAGE ns_storage
        TYPE filesystem
        NAME 'Namespace Storage'
        PATH '/data/ns_storage'
        SHARED_TABLES_TEMPLATE '/data/ns_storage/shared/{namespace}/{tableName}'
    "#;
    server.execute_sql(create_storage).await;

    // Create namespace with storage reference
    fixtures::create_namespace(&server, "storage_ns").await;

    // Create shared table in namespace (implicitly uses namespace's storage or default)
    let create_table = r#"
        CREATE TABLE storage_ns.shared_data (
            id BIGINT,
            data TEXT
        ) TABLE_TYPE shared
    "#;

    let response = server.execute_sql(create_table).await;
    assert_eq!(
        response.status, "success",
        "CREATE shared table failed: {:?}",
        response.error
    );

    // Verify table exists
    let query =
        "SELECT * FROM system.tables WHERE namespace = 'storage_ns' AND table_name = 'shared_data'";
    let response = server.execute_sql(query).await;

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected 1 shared table");
        let table = &rows[0];
        assert_eq!(
            table.get("table_type").and_then(|v| v.as_str()),
            Some("shared"),
            "Table type should be 'shared'"
        );
    }
}
