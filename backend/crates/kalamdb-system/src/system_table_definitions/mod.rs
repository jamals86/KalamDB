pub mod jobs;
pub mod live_queries;
pub mod manifest;
pub mod namespaces;
pub mod storages;
pub mod tables;
pub mod users;

pub use jobs::jobs_table_definition;
pub use live_queries::live_queries_table_definition;
pub use manifest::manifest_table_definition;
pub use namespaces::namespaces_table_definition;
pub use storages::storages_table_definition;
pub use tables::tables_table_definition;
pub use users::users_table_definition;

use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableDefinition;

/// Get TableId for a system table
pub fn system_table_id(table_name: &str) -> TableId {
    TableId::from_strings("system", table_name)
}

/// Get all system table definitions
pub fn all_system_table_definitions() -> Vec<(TableId, TableDefinition)> {
    vec![
        (system_table_id("users"), users_table_definition()),
        (system_table_id("jobs"), jobs_table_definition()),
        (system_table_id("namespaces"), namespaces_table_definition()),
        (system_table_id("storages"), storages_table_definition()),
        (
            system_table_id("live_queries"),
            live_queries_table_definition(),
        ),
        (system_table_id("tables"), tables_table_definition()),
        (system_table_id("manifest"), manifest_table_definition()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::datatypes::KalamDataType;
    use kalamdb_commons::schemas::TableType;

    #[test]
    fn test_users_table_definition() {
        let def = users_table_definition();
        assert_eq!(def.namespace_id.as_str(), "system");
        assert_eq!(def.table_name.as_str(), "users");
        assert_eq!(def.table_type, TableType::System);
        assert_eq!(def.columns.len(), 13);
        assert_eq!(def.columns[0].column_name, "user_id");
        assert!(def.columns[0].is_primary_key);
    }

    #[test]
    fn test_all_system_tables() {
        let all_tables = all_system_table_definitions();
        assert_eq!(all_tables.len(), 7);

        // Verify all tables are in system namespace
        for (table_id, def) in all_tables {
            assert_eq!(table_id.namespace_id().as_str(), "system");
            assert_eq!(def.namespace_id.as_str(), "system");
            assert_eq!(def.table_type, TableType::System);
        }
    }

}
