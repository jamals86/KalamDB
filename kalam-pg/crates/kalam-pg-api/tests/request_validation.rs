use datafusion::scalar::ScalarValue;
use kalam_pg_api::{InsertRequest, ScanRequest, TenantContext};
use kalamdb_commons::models::rows::Row;
use kalamdb_commons::models::{NamespaceId, TableName, UserId};
use kalamdb_commons::{TableId, TableType};

#[test]
fn user_table_scan_requires_user_id() {
    let request = ScanRequest::new(
        TableId::new(NamespaceId::new("app"), TableName::new("messages")),
        TableType::User,
        TenantContext::anonymous(),
    );

    let err = request.validate().expect_err("user tables should require user context");
    assert!(err.to_string().contains("user_id"));
}

#[test]
fn shared_table_scan_allows_missing_user_id() {
    let request = ScanRequest::new(
        TableId::new(NamespaceId::new("app"), TableName::new("messages")),
        TableType::Shared,
        TenantContext::anonymous(),
    );

    assert!(request.validate().is_ok());
}

#[test]
fn tenant_context_uses_explicit_user_id() {
    let tenant = TenantContext::with_user_id(UserId::new("u_123"));
    assert_eq!(tenant.effective_user_id(), Some(&UserId::new("u_123")));
}

#[test]
fn user_table_insert_requires_user_id() {
    let request = InsertRequest::new(
        TableId::new(NamespaceId::new("app"), TableName::new("messages")),
        TableType::User,
        TenantContext::anonymous(),
        vec![Row::from_vec(vec![(
            "id".to_string(),
            ScalarValue::Int32(Some(1)),
        )])],
    );

    let err = request
        .validate()
        .expect_err("user table inserts should require user context");
    assert!(err.to_string().contains("user_id"));
}

#[test]
fn insert_requires_rows() {
    let request = InsertRequest::new(
        TableId::new(NamespaceId::new("app"), TableName::new("messages")),
        TableType::Shared,
        TenantContext::anonymous(),
        Vec::new(),
    );

    let err = request
        .validate()
        .expect_err("insert requests without rows should fail validation");
    assert!(err.to_string().contains("at least one row"));
}
