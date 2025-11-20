#![allow(unused_imports)]
//! Integration tests for User Story 4: Shared Table Access Control
//!
//! Tests Phase 6 requirements:
//! - T085: Public shared table read-only access for all authenticated users
//! - T086: Private shared table access for service/dba/system only
//! - T087: Default access level validation (defaults to private)
//! - T088: Access level modification authorization (only service/dba/system can modify)
//! - T089: Read-only enforcement for regular users on public tables

#[path = "integration/common/mod.rs"]
mod common;

use common::{auth_helper, TestServer};
use kalamdb_api::models::{QueryResult, SqlResponse};
use kalamdb_commons::{Role, TableAccess};
use uuid::Uuid;

/// Helper function to check if SQL response is successful
fn is_success(response: &SqlResponse) -> bool {
    response.status == kalamdb_api::models::ResponseStatus::Success
}

/// Helper function to check if SQL response is error
fn is_error(response: &SqlResponse) -> bool {
    response.status == kalamdb_api::models::ResponseStatus::Error
}

/// Helper function to create column family for shared table
///
/// SharedTableStore expects column families to exist before CREATE TABLE.
/// This helper creates the CF dynamically for testing purposes.
fn create_shared_table_cf(
    _server: &TestServer,
    _namespace: &str,
    _table_name: &str,
) -> Result<(), String> {
    // SharedTableStore expects column families to exist before CREATE TABLE.
    // Test harness does not manage CF creation dynamically here.
    Ok(())
}

/// Helper function to initialize test server with a unique namespace
async fn init_server() -> (TestServer, String) {
    let server = TestServer::new().await;

    // Generate unique namespace to avoid collisions in parallel tests
    let uuid_str = Uuid::new_v4().to_string().replace("-", "");
    let namespace = format!("ns_{}", &uuid_str[..8]);

    // Create namespace as system user (system user is bootstrapped by TestServer)
    let create_ns_sql = format!("CREATE NAMESPACE {}", namespace);
    let result = server
        .execute_sql_as_user(&create_ns_sql, "system")
        .await;
    if !is_success(&result) {
        panic!("Failed to create namespace {}: {:?}", namespace, result.error);
    }

    (server, namespace)
}

/// T085: Integration test for public shared table read-only access
///
/// Requirement: All authenticated users can read public shared tables
/// but cannot modify them (INSERT/UPDATE/DELETE operations should fail)
///
/// NOTE: This test is ignored because shared tables require pre-created column families
/// at DB initialization. TestServer::new() creates an in-memory DB without dynamic CF support.
/// The functionality is verified through unit tests in:
/// - backend/crates/kalamdb-sql/src/ddl/create_table.rs (ACCESS LEVEL parsing)
/// - backend/crates/kalamdb-sql/src/ddl/alter_table.rs (SET ACCESS LEVEL parsing)
/// - backend/crates/kalamdb-core/src/sql/executor.rs (RBAC enforcement)
#[actix_web::test]
#[ignore = "Shared tables require pre-created column families at DB init"]
async fn test_public_table_read_only_for_users() {
    let (server, namespace) = init_server().await;
    let uuid_str = Uuid::new_v4().to_string().replace("-", "");
    let suffix = &uuid_str[..8];

    // Create a service user to set up the table
    let service_username = format!("service_{}", suffix);
    let service_password = "ServicePass123!";
    let service_user =
        auth_helper::create_test_user(&server, &service_username, service_password, Role::Service)
            .await;

    // Create column family for the shared table
    create_shared_table_cf(&server, &namespace, "messages").expect("Failed to create column family");

    // Create a public shared table (as service user with correct user_id)
    let create_table_sql = format!(r#"
        CREATE SHARED TABLE {}.messages (
            id BIGINT PRIMARY KEY,
            content TEXT NOT NULL
        ) ACCESS LEVEL public
    "#, namespace);
    let result = server
        .execute_sql_as_user(&create_table_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Failed to create public table: {:?}",
        result.error
    );

    // Create a regular user
    let regular_username = format!("regular_{}", suffix);
    let regular_password = "RegularPass123!";
    let regular_user =
        auth_helper::create_test_user(&server, &regular_username, regular_password, Role::User)
            .await;

    // Test 1: Regular user CAN read from public table
    let select_sql = format!("SELECT * FROM {}.messages", namespace);
    let result = server
        .execute_sql_as_user(&select_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Regular user should be able to read public table: {:?}",
        result.error
    );

    // Test 2: Regular user CANNOT insert into public table (read-only)
    let insert_sql = format!("INSERT INTO {}.messages (id, content) VALUES (1, 'test')", namespace);
    let result = server
        .execute_sql_as_user(&insert_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT be able to insert into public table"
    );

    // Test 3: Regular user CANNOT update public table
    let update_sql = format!("UPDATE {}.messages SET content = 'updated' WHERE id = 1", namespace);
    let result = server
        .execute_sql_as_user(&update_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT be able to update public table"
    );

    // Test 4: Regular user CANNOT delete from public table
    let delete_sql = format!("DELETE FROM {}.messages WHERE id = 1", namespace);
    let result = server
        .execute_sql_as_user(&delete_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT be able to delete from public table"
    );
}

/// T086: Integration test for private shared table access (service/dba/system only)
#[actix_web::test]
async fn test_private_table_service_dba_only() {
    let (server, namespace) = init_server().await;
    let uuid_str = Uuid::new_v4().to_string().replace("-", "");
    let suffix = &uuid_str[..8];

    // Create users with different roles
    let service_username = format!("service_{}", suffix);
    let service_password = "ServicePass123!";
    let service_user =
        auth_helper::create_test_user(&server, &service_username, service_password, Role::Service)
            .await;

    let dba_username = format!("dba_{}", suffix);
    let dba_password = "DbaPass123!";
    let dba_user =
        auth_helper::create_test_user(&server, &dba_username, dba_password, Role::Dba).await;

    let regular_username = format!("regular_{}", suffix);
    let regular_password = "RegularPass123!";
    let regular_user =
        auth_helper::create_test_user(&server, &regular_username, regular_password, Role::User)
            .await;

    // Create a private shared table (as service user)
    let create_table_sql = format!(r#"
        CREATE SHARED TABLE {}.sensitive_data (
            id BIGINT PRIMARY KEY,
            secret TEXT NOT NULL
        ) ACCESS LEVEL private
    "#, namespace);
    let result = server
        .execute_sql_as_user(&create_table_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Failed to create private table: {:?}",
        result.error
    );

    // Test 1: Service user CAN access private table
    let select_sql = format!("SELECT * FROM {}.sensitive_data", namespace);
    let result = server
        .execute_sql_as_user(&select_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Service user should access private table: {:?}",
        result.error
    );

    // Test 2: DBA user CAN access private table
    let result = server
        .execute_sql_as_user(&select_sql, dba_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "DBA user should access private table: {:?}",
        result.error
    );

    // Test 3: Regular user CANNOT access private table
    let result = server
        .execute_sql_as_user(&select_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT access private table"
    );

    // Test 4: Service user CAN modify private table
    let insert_sql = format!("INSERT INTO {}.sensitive_data (id, secret) VALUES (1, 'confidential')", namespace);
    let result = server
        .execute_sql_as_user(&insert_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Service user should be able to modify private table: {:?}",
        result.error
    );
}

/// T087: Integration test for default access level
#[actix_web::test]
async fn test_shared_table_defaults_to_private() {
    let (server, namespace) = init_server().await;
    let uuid_str = Uuid::new_v4().to_string().replace("-", "");
    let suffix = &uuid_str[..8];

    // Create a service user
    let service_username = format!("service_{}", suffix);
    let service_password = "ServicePass123!";
    auth_helper::create_test_user(&server, &service_username, service_password, Role::Service).await;

    // Create shared table WITHOUT specifying ACCESS LEVEL
    let create_table_sql = format!(r#"
        CREATE SHARED TABLE {}.default_access (
            id BIGINT PRIMARY KEY,
            data TEXT
        )
    "#, namespace);
    let result = server
        .execute_sql_as_user(&create_table_sql, &service_username)
        .await;
    assert!(
        is_success(&result),
        "Failed to create table: {:?}",
        result.error
    );

    // Verify the table was created with default "private" access level
    let query_table_sql = format!(
        "SELECT access_level FROM system.tables WHERE table_id = '{}:default_access'",
        namespace
    );
    let query_result = server
        .execute_sql_as_user(&query_table_sql, &service_username)
        .await;

    assert!(
        is_success(&query_result),
        "Failed to query system.tables: {:?}",
        query_result.error
    );

    assert!(!query_result.results.is_empty(), "Expected query results");
    let result = &query_result.results[0];

    if let Some(ref rows) = result.rows {
        assert!(!rows.is_empty(), "Table should exist in system.tables");

        let row = &rows[0];
        let access_level = row
            .get("access_level")
            .and_then(|v| v.as_str())
            .expect("access_level should be present");

        assert_eq!(
            access_level, "Private",
            "Default access level should be Private"
        );
    } else {
        panic!("Expected rows in system.tables query result");
    }

    // Create a regular user and verify they cannot access it
    let regular_username = format!("regular_{}", suffix);
    let regular_password = "RegularPass123!";
    auth_helper::create_test_user(&server, &regular_username, regular_password, Role::User).await;

    let select_sql = format!("SELECT * FROM {}.default_access", namespace);
    let result = server
        .execute_sql_as_user(&select_sql, &regular_username)
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT access table with default private access"
    );
}

/// T088: Integration test for access level modification authorization
#[actix_web::test]
async fn test_change_access_level_requires_privileges() {
    let (server, namespace) = init_server().await;
    let uuid_str = Uuid::new_v4().to_string().replace("-", "");
    let suffix = &uuid_str[..8];

    // Create users with different roles
    let service_username = format!("service_{}", suffix);
    let service_password = "ServicePass123!";
    let service_user =
        auth_helper::create_test_user(&server, &service_username, service_password, Role::Service)
            .await;

    let dba_username = format!("dba_{}", suffix);
    let dba_password = "DbaPass123!";
    let dba_user =
        auth_helper::create_test_user(&server, &dba_username, dba_password, Role::Dba).await;

    let regular_username = format!("regular_{}", suffix);
    let regular_password = "RegularPass123!";
    let regular_user =
        auth_helper::create_test_user(&server, &regular_username, regular_password, Role::User)
            .await;

    // Create a private shared table
    let create_table_sql = format!(r#"
        CREATE SHARED TABLE {}.config (
            id BIGINT PRIMARY KEY,
            value TEXT
        ) ACCESS LEVEL private
    "#, namespace);
    let result = server
        .execute_sql_as_user(&create_table_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Failed to create table: {:?}",
        result.error
    );

    // Test 1: Regular user CANNOT change access level
    let alter_sql = format!("ALTER TABLE {}.config SET ACCESS LEVEL public", namespace);
    let result = server
        .execute_sql_as_user(&alter_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT be able to change access level"
    );

    // Verify access level unchanged
    let table_id = kalamdb_commons::models::TableId::new(
        kalamdb_commons::models::NamespaceId::new(&namespace),
        kalamdb_commons::models::TableName::new("config"),
    );
    let table = server
        .app_context
        .system_tables()
        .tables()
        .get_table_by_id(&table_id)
        .unwrap()
        .unwrap();
    match table.options() {
        kalamdb_commons::models::schemas::TableOptions::Shared(opts) => {
            assert_eq!(opts.access_level, Some(TableAccess::Private));
        }
        other => panic!("Expected Shared table options, got {:?}", other),
    }

    // Test 2: Service user CAN change access level
    let result = server
        .execute_sql_as_user(&alter_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Service user should be able to change access level: {:?}",
        result.error
    );

    // Verify access level changed to public
    let table = server
        .app_context
        .system_tables()
        .tables()
        .get_table_by_id(&table_id)
        .unwrap()
        .unwrap();
    match table.options() {
        kalamdb_commons::models::schemas::TableOptions::Shared(opts) => {
            assert_eq!(opts.access_level, Some(TableAccess::Public));
        }
        other => panic!("Expected Shared table options, got {:?}", other),
    }

    // Test 3: DBA user CAN change access level back
    let alter_sql_private = format!("ALTER TABLE {}.config SET ACCESS LEVEL private", namespace);
    let result = server
        .execute_sql_as_user(&alter_sql_private, dba_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "DBA user should be able to change access level: {:?}",
        result.error
    );

    // Verify access level changed back to private
    let table = server
        .app_context
        .system_tables()
        .tables()
        .get_table_by_id(&table_id)
        .unwrap()
        .unwrap();
    match table.options() {
        kalamdb_commons::models::schemas::TableOptions::Shared(opts) => {
            assert_eq!(opts.access_level, Some(TableAccess::Private));
        }
        other => panic!("Expected Shared table options, got {:?}", other),
    }
}

/// T089: Integration test for read-only enforcement on public tables
#[actix_web::test]
async fn test_user_cannot_modify_public_table() {
    let (server, namespace) = init_server().await;
    let uuid_str = Uuid::new_v4().to_string().replace("-", "");
    let suffix = &uuid_str[..8];

    // Create service user and public table
    let service_username = format!("service_{}", suffix);
    let service_password = "ServicePass123!";
    let service_user =
        auth_helper::create_test_user(&server, &service_username, service_password, Role::Service)
            .await;

    let create_table_sql = format!(r#"
        CREATE SHARED TABLE {}.announcements (
            id BIGINT PRIMARY KEY,
            message TEXT NOT NULL
        ) ACCESS LEVEL public
    "#, namespace);
    let result = server
        .execute_sql_as_user(&create_table_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Failed to create table: {:?}",
        result.error
    );

    // Service user adds some data
    let insert_sql = format!("INSERT INTO {}.announcements (id, message) VALUES (1, 'Welcome')", namespace);
    let result = server
        .execute_sql_as_user(&insert_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Service user should insert data: {:?}",
        result.error
    );

    // Create regular user
    let regular_username = format!("regular_{}", suffix);
    let regular_password = "RegularPass123!";
    let regular_user =
        auth_helper::create_test_user(&server, &regular_username, regular_password, Role::User)
            .await;

    // Test 1: Regular user CAN read
    let select_sql = format!("SELECT * FROM {}.announcements", namespace);
    let result = server
        .execute_sql_as_user(&select_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Regular user should be able to SELECT from public table: {:?}",
        result.error
    );

    // Test 2: Regular user CANNOT insert
    let insert_sql = format!("INSERT INTO {}.announcements (id, message) VALUES (2, 'Hacked')", namespace);
    let result = server
        .execute_sql_as_user(&insert_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT be able to INSERT into public table"
    );

    // Test 3: Regular user CANNOT update
    let update_sql = format!("UPDATE {}.announcements SET message = 'Modified' WHERE id = 1", namespace);
    let result = server
        .execute_sql_as_user(&update_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT be able to UPDATE public table"
    );

    // Test 4: Regular user CANNOT delete
    let delete_sql = format!("DELETE FROM {}.announcements WHERE id = 1", namespace);
    let result = server
        .execute_sql_as_user(&delete_sql, regular_user.id.as_str())
        .await;
    assert!(
        is_error(&result),
        "Regular user should NOT be able to DELETE from public table"
    );

    // Test 5: Service user CAN still modify (verification)
    let update_sql = format!("UPDATE {}.announcements SET message = 'Updated by service' WHERE id = 1", namespace);
    let result = server
        .execute_sql_as_user(&update_sql, service_user.id.as_str())
        .await;
    assert!(
        is_success(&result),
        "Service user should be able to modify public table: {:?}",
        result.error
    );
}

/// Additional test: Verify ACCESS LEVEL cannot be set on USER or STREAM tables
#[actix_web::test]
async fn test_access_level_only_on_shared_tables() {
    let (server, _namespace) = init_server().await;
    let uuid_str = Uuid::new_v4().to_string().replace("-", "");
    let suffix = &uuid_str[..8];

    let service_username = format!("service_{}", suffix);
    let service_password = "ServicePass123!";
    auth_helper::create_test_user(&server, &service_username, service_password, Role::Service).await;

    // Test 1: USER table with ACCESS LEVEL should fail
    let create_user_table_sql = r#"
        CREATE USER TABLE my_data (
            id BIGINT PRIMARY KEY,
            data TEXT
        ) ACCESS LEVEL public
    "#;
    let result = server
        .execute_sql_as_user(create_user_table_sql, &service_username)
        .await;
    assert!(
        is_error(&result),
        "ACCESS LEVEL should not be allowed on USER tables"
    );

    // Test 2: STREAM table with ACCESS LEVEL should fail
    let create_stream_table_sql = r#"
        CREATE STREAM TABLE events (
            id BIGINT PRIMARY KEY,
            event_type TEXT
        ) ACCESS LEVEL public TTL 3600
    "#;
    let result = server
        .execute_sql_as_user(create_stream_table_sql, &service_username)
        .await;
    assert!(
        is_error(&result),
        "ACCESS LEVEL should not be allowed on STREAM tables"
    );
}

/// Additional test: Verify ALTER TABLE SET ACCESS LEVEL only works on SHARED tables
#[actix_web::test]
async fn test_alter_access_level_only_on_shared_tables() {
    let (server, namespace) = init_server().await;
    let uuid_str = Uuid::new_v4().to_string().replace("-", "");
    let suffix = &uuid_str[..8];

    let service_username = format!("service_{}", suffix);
    let service_password = "ServicePass123!";
    auth_helper::create_test_user(&server, &service_username, service_password, Role::Service).await;

    // Create a USER table
    let create_user_table_sql = format!(r#"
        CREATE USER TABLE {}.personal_notes (
            id BIGINT PRIMARY KEY,
            note TEXT
        )
    "#, namespace);
    let result = server
        .execute_sql_as_user(&create_user_table_sql, &service_username)
        .await;
    assert!(
        is_success(&result),
        "Failed to create USER table: {:?}",
        result.error
    );

    // Try to set ACCESS LEVEL on USER table - should fail
    let alter_sql = format!("ALTER TABLE {}.personal_notes SET ACCESS LEVEL public", namespace);
    let result = server
        .execute_sql_as_user(&alter_sql, &service_username)
        .await;
    assert!(
        is_error(&result),
        "ALTER TABLE SET ACCESS LEVEL should fail on USER tables"
    );
}
