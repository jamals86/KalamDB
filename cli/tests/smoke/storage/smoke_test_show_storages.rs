// Smoke test: SHOW STORAGES command validation
// - Verifies SHOW STORAGES returns at least 'local' storage
// - Validates all expected columns are present
// - Checks data types and non-empty values for required fields

use crate::common::*;
use serde_json::Value as JsonValue;

#[ntest::timeout(60_000)]
#[test]
fn smoke_show_storages_basic() {
    if !is_server_running() {
        println!(
            "Skipping smoke_show_storages_basic: server not running at {}",
            server_url()
        );
        return;
    }

    let sql = "SHOW STORAGES";
    let result = execute_sql_as_root_via_client(sql).expect("SHOW STORAGES should succeed");

    // Parse the JSON result
    let json: JsonValue = serde_json::from_str(&result)
        .unwrap_or_else(|err| panic!("Failed to parse JSON response: {}\n{}", err, result));

    let rows = get_rows_as_hashmaps(&json).unwrap_or_default();
    assert!(
        !rows.is_empty(),
        "SHOW STORAGES should return at least 1 row (local storage)"
    );

    // Find the 'local' storage
    let local_storage = rows
        .iter()
        .find(|row| {
            row.get("storage_id")
                .and_then(|v| extract_arrow_value(v).unwrap_or_else(|| v.clone()).as_str())
                == Some("local")
        })
        .expect("'local' storage should be present in SHOW STORAGES output");

    // Verify required columns for local storage
    let storage_name = local_storage
        .get("storage_name")
        .and_then(|v| extract_arrow_value(v).unwrap_or_else(|| v.clone()).as_str())
        .map(String::from);
    assert!(
        storage_name.is_some() && !storage_name.as_ref().unwrap().is_empty(),
        "storage_name should be non-empty"
    );

    let storage_type = local_storage
        .get("storage_type")
        .and_then(|v| extract_arrow_value(v).unwrap_or_else(|| v.clone()).as_str())
        .map(String::from);
    assert_eq!(
        storage_type.as_deref(),
        Some("filesystem"),
        "local storage should be filesystem type"
    );

    let base_directory = local_storage
        .get("base_directory")
        .and_then(|v| extract_arrow_value(v).unwrap_or_else(|| v.clone()).as_str())
        .map(String::from);
    assert!(
        base_directory.is_some() && !base_directory.as_ref().unwrap().is_empty(),
        "base_directory should be non-empty for filesystem storage"
    );

    // Verify timestamps are present and reasonable
    let created_at = local_storage
        .get("created_at")
        .and_then(|v| extract_arrow_value(v).unwrap_or_else(|| v.clone()).as_str())
        .map(String::from);
    assert!(
        created_at.is_some(),
        "created_at should be present"
    );

    let updated_at = local_storage
        .get("updated_at")
        .and_then(|v| extract_arrow_value(v).unwrap_or_else(|| v.clone()).as_str())
        .map(String::from);
    assert!(
        updated_at.is_some(),
        "updated_at should be present"
    );
}

#[ntest::timeout(60_000)]
#[test]
fn smoke_show_storages_user_access() {
    if !is_server_running() {
        println!(
            "Skipping smoke_show_storages_user_access: server not running at {}",
            server_url()
        );
        return;
    }

    // Create a regular user to test authorization
    let test_user = generate_unique_namespace("show_user");
    let test_password = "ShowPass123!";

    let cleanup_user = || {
        let _ = execute_sql_as_root_via_client(&format!("DROP USER IF EXISTS '{}'", test_user));
    };
    let _cleanup_guard = CallOnDrop::new(cleanup_user);

    // Create user
    execute_sql_as_root_via_client(&format!(
        "CREATE USER {} WITH PASSWORD '{}' ROLE 'user'",
        test_user, test_password
    ))
    .expect("create test user");

    // Regular user SHOULD be able to run SHOW STORAGES (read-only operation)
    let sql = "SHOW STORAGES";
    let result = execute_sql_via_client_as(&test_user, test_password, sql);

    assert!(
        result.is_ok(),
        "Regular user should be able to run SHOW STORAGES"
    );

    let json: JsonValue = serde_json::from_str(&result.unwrap())
        .expect("Should parse JSON response");

    let rows = get_rows_as_hashmaps(&json).unwrap_or_default();
    assert!(
        !rows.is_empty(),
        "User should see at least the local storage"
    );
}

// Helper struct for cleanup on drop
struct CallOnDrop<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> CallOnDrop<F> {
    fn new(f: F) -> Self {
        CallOnDrop(Some(f))
    }
}

impl<F: FnOnce()> Drop for CallOnDrop<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}
