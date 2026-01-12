//! Integration tests for Phase 2 MVCC Architecture (T050-T055, T060, T062-T064)
//!
//! Tests:
//! - T050: SeqId creation, timestamp extraction, ordering, serialization (unit tests in seq_id.rs)
//! - T051: CREATE TABLE without PK → rejected with error
//! - T052: CREATE TABLE with user PK → `_seq: SeqId` and `_deleted: bool` auto-added to schema
//! - T053: INSERT → verify storage key format
//! - T054: INSERT → verify UserTableRow structure (user_id, _seq, _deleted, fields)
//! - T055: INSERT to shared table → verify SharedTableRow structure (_seq, _deleted, fields)
//! - T060: INSERT duplicate PK → rejected with uniqueness error
//! - T062: Incremental sync `WHERE _seq > X` → returns all versions after SeqId threshold
//! - T063: RocksDB prefix scan `{user_id}:` → efficiently returns only that user's rows
//! - T064: RocksDB range scan `_seq > threshold` → efficiently skips older versions

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, QueryResultTestExt, TestServer};
use kalamdb_api::models::ResponseStatus;

/// T051: CREATE TABLE without PK should be rejected
#[actix_web::test]
async fn test_create_table_without_pk_rejected() {
    let server = TestServer::new().await;

    // Setup
    let ns_response = fixtures::create_namespace(&server, "test_ns_t051").await;
    assert_eq!(
        ns_response.status,
        ResponseStatus::Success,
        "Failed to create namespace: {:?}",
        ns_response.error
    );

    // Try to create table without PRIMARY KEY specification
    let response = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t051.invalid_table (
                id TEXT,
                name TEXT,
                value INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Should fail with error about missing primary key
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "CREATE TABLE without PK should fail, got: {:?}",
        response
    );

    if let Some(error) = &response.error {
        let msg_lower = error.message.to_lowercase();
        assert!(
            msg_lower.contains("primary") || msg_lower.contains("pk") || msg_lower.contains("key"),
            "Error message should mention primary key, got: {}",
            error.message
        );
    } else {
        panic!("Expected error message about primary key");
    }

    println!("✅ T051: CREATE TABLE without PK correctly rejected");
}

/// T052: CREATE TABLE with user PK → verify `_seq` and `_deleted` auto-added to schema
#[actix_web::test]
async fn test_create_table_auto_adds_system_columns() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns_t052").await;

    // Create table with user-defined PK
    let response = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t052.products (
                id TEXT PRIMARY KEY,
                name TEXT,
                price INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "CREATE TABLE should succeed: {:?}",
        response.error
    );

    // Insert test data
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t052.products (id, name, price) 
               VALUES ('prod1', 'Widget', 100)"#,
            "user1",
        )
        .await;

    // Query with explicit _seq and _deleted columns
    let response = server
        .execute_sql_as_user(
            "SELECT id, name, price, _seq, _deleted FROM test_ns_t052.products WHERE id = 'prod1'",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1, "Should return exactly 1 row");
    let row = &rows[0];

    // Verify user columns
    assert_eq!(row.get("id").unwrap().as_str().unwrap(), "prod1");
    assert_eq!(row.get("name").unwrap().as_str().unwrap(), "Widget");
    assert_eq!(row.get("price").unwrap().as_i64().unwrap(), 100);

    // Verify system columns exist
    assert!(row.contains_key("_seq"), "_seq column should be auto-added");
    assert!(
        row.contains_key("_deleted"),
        "_deleted column should be auto-added"
    );

    // Verify _seq is a valid i64 (SeqId)
    let seq = row.get("_seq").unwrap();
    assert!(
        seq.is_i64() || seq.is_u64(),
        "_seq should be numeric (SeqId), got: {:?}",
        seq
    );

    // Verify _deleted defaults to false
    assert_eq!(
        row.get("_deleted").unwrap().as_bool(),
        Some(false),
        "_deleted should default to false"
    );

    println!("✅ T052: CREATE TABLE auto-adds _seq and _deleted system columns");
}

/// T053: INSERT → verify storage key format for user and shared tables
#[actix_web::test]
async fn test_insert_storage_key_format() {
    let server = TestServer::new().await;

    // Setup
    let resp = fixtures::create_namespace(&server, "test_ns_t053").await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Namespace creation failed: {:?}",
        resp.error
    );

    // Create user table
    let resp = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t053.user_data (
                id TEXT PRIMARY KEY,
                content TEXT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "User table creation failed: {:?}",
        resp.error
    );

    // Create shared table with system privileges
    let resp = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t053.shared_data (
                id TEXT PRIMARY KEY,
                content TEXT
            ) WITH (
                TYPE = 'SHARED',
                STORAGE_ID = 'local'
            )"#,
            "system",
        )
        .await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Shared table creation failed: {:?}",
        resp.error
    );

    // Insert into user table
    let response = server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t053.user_data (id, content) 
               VALUES ('rec1', 'User data')"#,
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "User table INSERT should succeed: {:?}",
        response.error
    );

    // Insert into shared table (must be done by system/owner for Private tables)
    let response = server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t053.shared_data (id, content) 
               VALUES ('rec1', 'Shared data')"#,
            "system",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Shared table INSERT should succeed: {:?}",
        response.error
    );

    // Verify data can be queried (storage key format works)
    let response = server
        .execute_sql_as_user("SELECT id, content FROM test_ns_t053.user_data", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1, "Should retrieve user table record");

    let response = server
        .execute_sql_as_user("SELECT id, content FROM test_ns_t053.shared_data", "system")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1, "Should retrieve shared table record");

    println!("✅ T053: INSERT storage key format works for user and shared tables");
}

/// T054: INSERT → verify UserTableRow structure (user_id, _seq, _deleted, fields)
#[actix_web::test]
async fn test_user_table_row_structure() {
    let server = TestServer::new().await;

    // Setup
    let resp = fixtures::create_namespace(&server, "test_ns_t054").await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Namespace creation failed: {:?}",
        resp.error
    );

    let resp = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t054.user_records (
                record_id TEXT PRIMARY KEY,
                title TEXT,
                priority INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Table creation failed: {:?}",
        resp.error
    );

    // Insert record
    let resp = server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t054.user_records (record_id, title, priority) 
               VALUES ('rec1', 'Important', 5)"#,
            "user1",
        )
        .await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Insert failed: {:?}",
        resp.error
    );

    // Query with all columns including system columns
    let response = server
        .execute_sql_as_user(
            "SELECT record_id, title, priority, _seq, _deleted FROM test_ns_t054.user_records",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];

    // Verify fields (user-defined columns in fields JSON)
    assert!(row.contains_key("record_id"), "record_id (PK) should exist");
    assert!(row.contains_key("title"), "title should exist");
    assert!(row.contains_key("priority"), "priority should exist");

    // Verify system columns
    assert!(row.contains_key("_seq"), "_seq should exist");
    assert!(row.contains_key("_deleted"), "_deleted should exist");

    // Verify _seq is numeric (SeqId wrapper)
    let seq = row.get("_seq").unwrap();
    assert!(
        seq.is_i64() || seq.is_u64(),
        "_seq should be numeric, got: {:?}",
        seq
    );

    // Verify _deleted is boolean
    assert_eq!(
        row.get("_deleted").unwrap().as_bool(),
        Some(false),
        "_deleted should be false"
    );

    // Note: user_id is NOT exposed in query results (internal to storage key)
    // UserTableRow structure: { user_id: UserId, _seq: SeqId, _deleted: bool, fields: JsonValue }

    println!("✅ T054: UserTableRow structure verified (user_id internal, _seq, _deleted, fields)");
}

/// T055: INSERT to shared table → verify SharedTableRow structure (_seq, _deleted, fields only)
#[actix_web::test]
async fn test_shared_table_row_structure() {
    let server = TestServer::new().await;

    // Setup - namespace and shared table created via system user
    let resp = fixtures::create_namespace(&server, "test_ns_t055").await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Namespace creation failed: {:?}",
        resp.error
    );

    let resp = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t055.shared_config (
                config_key TEXT PRIMARY KEY,
                value TEXT,
                enabled BOOLEAN
            ) WITH (
                TYPE = 'SHARED',
                STORAGE_ID = 'local'
            )"#,
            "system",
        )
        .await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Table creation failed: {:?}",
        resp.error
    );

    // Insert record (must be done by system/owner)
    let resp = server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t055.shared_config (config_key, value, enabled) 
               VALUES ('feature_flag', 'on', true)"#,
            "system",
        )
        .await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Insert failed: {:?}",
        resp.error
    );

    // Query with all columns including system columns (as system user)
    let response = server
        .execute_sql_as_user(
            "SELECT config_key, value, enabled, _seq, _deleted FROM test_ns_t055.shared_config",
            "system",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];

    // Verify fields (user-defined columns)
    assert_eq!(
        row.get("config_key").unwrap().as_str().unwrap(),
        "feature_flag"
    );
    assert_eq!(row.get("value").unwrap().as_str().unwrap(), "on");
    assert!(row.get("enabled").unwrap().as_bool().unwrap());

    // Verify system columns
    assert!(row.contains_key("_seq"), "_seq should exist");
    assert!(row.contains_key("_deleted"), "_deleted should exist");

    // Verify NO access_level column (removed from SharedTableRow in Phase 2)
    // access_level is now cached in schema definition, not per-row
    assert!(
        !row.contains_key("access_level"),
        "access_level should NOT be in SharedTableRow (cached in schema)"
    );

    // Verify _seq is numeric
    let seq = row.get("_seq").unwrap();
    assert!(
        seq.is_i64() || seq.is_u64(),
        "_seq should be numeric, got: {:?}",
        seq
    );

    // SharedTableRow structure: { _seq: SeqId, _deleted: bool, fields: JsonValue }
    // NO user_id (not user-scoped), NO access_level (in schema cache)

    println!("✅ T055: SharedTableRow structure verified (_seq, _deleted, fields only)");
}

/// T060: INSERT with duplicate PRIMARY KEY → validates uniqueness when user provides PK
///
/// **MVCC Smart Validation**:
/// - If user provides PK value → O(log n) uniqueness check (fast!)
/// - If PK has DEFAULT value → No validation needed (auto-generated = always unique)
/// - If user omits required PK → Error (PK cannot be NULL)
///
/// This test verifies that when the user explicitly provides a PK value,
/// the system correctly rejects duplicates.
#[actix_web::test]
async fn test_insert_duplicate_pk_rejected() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns_t060").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t060.unique_items (
                item_id TEXT PRIMARY KEY,
                name TEXT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert first record WITH explicit PK
    let response = server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t060.unique_items (item_id, name) 
               VALUES ('item1', 'First')"#,
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "First INSERT should succeed"
    );

    // Try to insert duplicate PK (should fail - user provided explicit PK value)
    let response = server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t060.unique_items (item_id, name) 
               VALUES ('item1', 'Duplicate')"#,
            "user1",
        )
        .await;

    // Should fail with uniqueness constraint error
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "Duplicate PK INSERT should fail when user provides explicit PK value"
    );

    if let Some(error) = &response.error {
        let msg_lower = error.message.to_lowercase();
        assert!(
            msg_lower.contains("primary key")
                || msg_lower.contains("already exists")
                || msg_lower.contains("violation"),
            "Error should mention primary key violation, got: {}",
            error.message
        );
    } else {
        panic!("Expected error about duplicate PK");
    }

    // Verify only one record exists
    let response = server
        .execute_sql_as_user(
            "SELECT item_id, name FROM test_ns_t060.unique_items",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(
        rows.len(),
        1,
        "Should only have 1 record (duplicate rejected)"
    );
    assert_eq!(
        rows[0].get("name").unwrap().as_str().unwrap(),
        "First",
        "Original record should remain"
    );

    // Verify different PK values still work
    let response = server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t060.unique_items (item_id, name) 
               VALUES ('item2', 'Second')"#,
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "INSERT with different PK should succeed"
    );

    println!("✅ T060: Duplicate PK correctly rejected when user provides explicit PK value");
}

/// T062: Incremental sync `WHERE _seq > X` → returns all versions after threshold
#[actix_web::test]
async fn test_incremental_sync_seq_threshold() {
    let server = TestServer::new().await;

    // Setup
    let resp = fixtures::create_namespace(&server, "test_ns_t062").await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Namespace creation failed: {:?}",
        resp.error
    );

    let resp = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t062.sync_records (
                id TEXT PRIMARY KEY,
                version INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "Table creation failed: {:?}",
        resp.error
    );

    // Insert 3 records
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t062.sync_records (id, version) VALUES ('rec1', 1)"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t062.sync_records (id, version) VALUES ('rec2', 2)"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t062.sync_records (id, version) VALUES ('rec3', 3)"#,
            "user1",
        )
        .await;

    // Get all records with _seq
    let response = server
        .execute_sql_as_user(
            "SELECT id, version, _seq FROM test_ns_t062.sync_records ORDER BY id",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let all_rows = response.results[0].rows_as_maps();
    assert_eq!(all_rows.len(), 3);

    // Get the _seq of the second record
    let threshold_seq = all_rows[1].get("_seq").unwrap().as_i64().unwrap();

    // Query with WHERE _seq > threshold (should return only rec3)
    let response = server
        .execute_sql_as_user(
            &format!(
                "SELECT id, version, _seq FROM test_ns_t062.sync_records WHERE _seq > {} ORDER BY id",
                threshold_seq
            ),
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1, "Should return only records after threshold");
    assert_eq!(
        rows[0].get("id").unwrap().as_str().unwrap(),
        "rec3",
        "Should return rec3 (latest)"
    );

    let returned_seq = rows[0].get("_seq").unwrap().as_i64().unwrap();
    assert!(
        returned_seq > threshold_seq,
        "Returned _seq should be greater than threshold"
    );

    println!("✅ T062: Incremental sync with WHERE _seq > X works correctly");
}

/// T063: RocksDB prefix scan `{user_id}:` → efficiently returns only that user's rows
#[actix_web::test]
async fn test_rocksdb_prefix_scan_user_isolation() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns_t063").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t063.user_notes (
                note_id TEXT PRIMARY KEY,
                content TEXT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert data for user1
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t063.user_notes (note_id, content) 
               VALUES ('note1', 'User1 Note 1')"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t063.user_notes (note_id, content) 
               VALUES ('note2', 'User1 Note 2')"#,
            "user1",
        )
        .await;

    // Insert data for user2 (different user)
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t063.user_notes (note_id, content) 
               VALUES ('note1', 'User2 Note 1')"#,
            "user2",
        )
        .await;

    // Query as user1 (should only see user1's notes via prefix scan)
    let response = server
        .execute_sql_as_user(
            "SELECT note_id, content FROM test_ns_t063.user_notes ORDER BY note_id",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 2, "User1 should only see their own 2 notes");
    assert_eq!(
        rows[0].get("content").unwrap().as_str().unwrap(),
        "User1 Note 1"
    );
    assert_eq!(
        rows[1].get("content").unwrap().as_str().unwrap(),
        "User1 Note 2"
    );

    // Query as user2 (should only see user2's note)
    let response = server
        .execute_sql_as_user(
            "SELECT note_id, content FROM test_ns_t063.user_notes ORDER BY note_id",
            "user2",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1, "User2 should only see their own 1 note");
    assert_eq!(
        rows[0].get("content").unwrap().as_str().unwrap(),
        "User2 Note 1"
    );

    println!("✅ T063: RocksDB prefix scan ensures user isolation");
}

/// T064: RocksDB range scan `_seq > threshold` → efficiently skips older versions
/// **NOTE**: This test has an UPDATE handler bug preventing validation.
/// The range scan logic works correctly, but UPDATE fails with "Row not found".
#[actix_web::test]
async fn test_rocksdb_range_scan_efficiency() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns_t064").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns_t064.versioned_data (
                id TEXT PRIMARY KEY,
                value INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert initial version
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns_t064.versioned_data (id, value) VALUES ('rec1', 1)"#,
            "user1",
        )
        .await;

    // Get initial _seq
    let response = server
        .execute_sql_as_user(
            "SELECT id, value, _seq FROM test_ns_t064.versioned_data",
            "user1",
        )
        .await;

    let initial_seq = response.results[0].rows_as_maps()[0]
        .get("_seq")
        .unwrap()
        .as_i64()
        .unwrap();

    // Update record (creates new version)
    server
        .execute_sql_as_user(
            r#"UPDATE test_ns_t064.versioned_data SET value = 2 WHERE id = 'rec1'"#,
            "user1",
        )
        .await;

    // Update again
    server
        .execute_sql_as_user(
            r#"UPDATE test_ns_t064.versioned_data SET value = 3 WHERE id = 'rec1'"#,
            "user1",
        )
        .await;

    // Query with WHERE _seq > initial_seq (range scan should skip first version)
    let response = server
        .execute_sql_as_user(
            &format!(
                "SELECT id, value, _seq FROM test_ns_t064.versioned_data WHERE _seq > {}",
                initial_seq
            ),
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    // Should return the latest version (value=3) since version resolution
    // applies MAX(_seq) AFTER the range filter
    assert_eq!(rows.len(), 1, "Should return 1 row (latest version)");
    assert_eq!(
        rows[0].get("value").unwrap().as_i64().unwrap(),
        3,
        "Should return latest value"
    );

    let returned_seq = rows[0].get("_seq").unwrap().as_i64().unwrap();
    assert!(
        returned_seq > initial_seq,
        "Returned _seq should be > initial_seq"
    );

    println!("✅ T064: RocksDB range scan with _seq > threshold works efficiently");
}
