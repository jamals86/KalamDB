//! Storage lifecycle integration tests
//!
//! Covers CREATE STORAGE / DROP STORAGE flows to ensure tables block deletion
//! until referencing tables are removed.

mod common;
use common::*;
use serde_json::Value as JsonValue;

/// Ensure DROP STORAGE fails while tables still reference the storage
#[test]
fn test_storage_drop_requires_detached_tables() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping storage lifecycle test.");
        return;
    }

    let unique_suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let storage_id = format!("cli_storage_drop_{}", unique_suffix);
    let namespace = generate_unique_namespace("storage_guard_ns");
    let user_table = format!("stor_user_{}", unique_suffix);
    let shared_table = format!("stor_shared_{}", unique_suffix);

    let temp_dir = TempDir::new().expect("create temp dir for storage path");
    let base_dir = temp_dir.path().join("storage_root");
    let base_dir_sql = base_dir
        .to_str()
        .expect("valid storage path")
        .replace('\'', "''");

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("namespace creation");

    let create_storage_sql = format!(
        "CREATE STORAGE {storage_id} \
            TYPE filesystem \
            NAME 'CLI Storage Test' \
            PATH '{base_dir}' \
            SHARED_TABLES_TEMPLATE 'ns_{{namespace}}/shared_{{tableName}}' \
            USER_TABLES_TEMPLATE 'ns_{{namespace}}/user_{{tableName}}/user_{{userId}}'",
        base_dir = base_dir_sql
    );
    execute_sql_as_root_via_cli(&create_storage_sql).expect("storage creation");

    assert!(
        base_dir.exists(),
        "filesystem storage should eagerly create its base directory"
    );

    let storage_rows = query_rows(&format!(
        "SELECT storage_id FROM system.storages WHERE storage_id = '{}'",
        storage_id
    ));
    assert_eq!(
        storage_rows.len(),
        1,
        "storage {} should be persisted in system.storages",
        storage_id
    );

    let create_user_table_sql = format!(
        "CREATE USER TABLE {}.{} (id INT AUTO_INCREMENT, body TEXT) STORAGE '{}' FLUSH ROWS 5",
        namespace, user_table, storage_id
    );
    execute_sql_as_root_via_cli(&create_user_table_sql).expect("user table creation");

    let creator_user = "root";
    let user_table_path = base_dir
        .join(format!("ns_{}", namespace))
        .join(format!("user_{}", user_table))
        .join(format!("user_{}", creator_user));
    assert!(
        user_table_path.exists(),
        "user table path should be created eagerly: {}",
        user_table_path.display()
    );

    let create_shared_table_sql = format!(
        "CREATE SHARED TABLE {}.{} (id INT AUTO_INCREMENT, body TEXT) STORAGE '{}' FLUSH ROWS 5",
        namespace, shared_table, storage_id
    );
    execute_sql_as_root_via_cli(&create_shared_table_sql).expect("shared table creation");

    let shared_table_path = base_dir
        .join(format!("ns_{}", namespace))
        .join(format!("shared_{}", shared_table));
    assert!(
        shared_table_path.exists(),
        "shared table path should be created eagerly: {}",
        shared_table_path.display()
    );

    let drop_err = execute_sql_as_root_via_cli(&format!("DROP STORAGE {}", storage_id));
    assert!(drop_err.is_err(), "drop storage should fail while tables exist");
    let err_msg = drop_err.err().unwrap().to_string();
    assert!(
        err_msg.contains("Cannot drop storage") && err_msg.contains("table(s) still using it"),
        "error message should mention storage still in use: {}",
        err_msg
    );

    execute_sql_as_root_via_cli(&format!("DROP TABLE {}.{}", namespace, user_table))
        .expect("drop user table");
    execute_sql_as_root_via_cli(&format!("DROP TABLE {}.{}", namespace, shared_table))
        .expect("drop shared table");

    execute_sql_as_root_via_cli(&format!("DROP STORAGE {}", storage_id))
        .expect("storage drop should succeed after tables removed");
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE {} CASCADE", namespace));
}

fn query_rows(sql: &str) -> Vec<JsonValue> {
    let output = execute_sql_as_root_via_cli_json(sql)
        .unwrap_or_else(|err| panic!("Failed to execute '{}': {}", sql, err));
    let json: JsonValue = serde_json::from_str(&output)
        .unwrap_or_else(|err| panic!("Failed to parse CLI JSON output: {}\n{}", err, output));
    json.get("results")
        .and_then(JsonValue::as_array)
        .and_then(|results| results.first())
        .and_then(|result| result.get("rows"))
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default()
}
