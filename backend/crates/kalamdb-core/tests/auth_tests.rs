use kalamdb_commons::{Role, TableType};
use kalamdb_core::auth::rbac::{can_access_table_type, can_create_table};

#[test]
fn test_can_access_table() {
    assert!(can_access_table_type(Role::User, TableType::User));
    assert!(can_access_table_type(Role::Service, TableType::Shared));
    assert!(!can_access_table_type(Role::User, TableType::System));
}

#[test]
fn test_can_create_table() {
    assert!(can_create_table(Role::Service, TableType::User));
    assert!(can_create_table(Role::Dba, TableType::System));
    assert!(!can_create_table(Role::User, TableType::System));
}
