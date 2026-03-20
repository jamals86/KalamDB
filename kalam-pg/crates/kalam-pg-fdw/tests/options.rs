use kalam_pg_fdw::{ServerOptions, TableOptions};
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_commons::{TableId, TableType};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn parses_embedded_server_options() {
    let options = BTreeMap::from([
        ("storage_base_path".to_string(), "/tmp/kalam-pg".to_string()),
        ("node_id".to_string(), "pg-node-1".to_string()),
    ]);

    let parsed = ServerOptions::parse(&options).expect("parse server options");

    assert_eq!(
        parsed.embedded_runtime.storage_base_path,
        PathBuf::from("/tmp/kalam-pg")
    );
    assert_eq!(parsed.embedded_runtime.node_id, "pg-node-1");
}

#[test]
fn parses_table_options() {
    let options = BTreeMap::from([
        ("namespace".to_string(), "app".to_string()),
        ("table".to_string(), "messages".to_string()),
        ("table_type".to_string(), "user".to_string()),
    ]);

    let parsed = TableOptions::parse(&options).expect("parse table options");

    assert_eq!(
        parsed.table_id,
        TableId::new(NamespaceId::new("app"), TableName::new("messages"))
    );
    assert_eq!(parsed.table_type, TableType::User);
}

#[test]
fn table_options_require_namespace() {
    let options = BTreeMap::from([
        ("table".to_string(), "messages".to_string()),
        ("table_type".to_string(), "user".to_string()),
    ]);

    let err = TableOptions::parse(&options).expect_err("missing namespace should fail");
    assert!(err.to_string().contains("namespace"));
}
