//! General helper utilities (moved from handlers/helpers.rs)

use crate::error::KalamDbError;
use crate::sql::executor::models::ExecutionContext;
use kalamdb_commons::models::TableId;
use kalamdb_commons::{NamespaceId, TableName};

pub fn resolve_namespace(
    statement_namespace: Option<&NamespaceId>,
    context: &ExecutionContext,
) -> NamespaceId {
    statement_namespace
        .cloned()
        .or_else(|| context.namespace_id().cloned())
        .unwrap_or_else(|| NamespaceId::from("default"))
}

pub fn resolve_namespace_required(
    statement_namespace: Option<&NamespaceId>,
    context: &ExecutionContext,
) -> Result<NamespaceId, KalamDbError> {
    statement_namespace
        .cloned()
        .or_else(|| context.namespace_id().cloned())
        .ok_or_else(|| {
            KalamDbError::InvalidOperation(
                "Namespace is required but not specified in statement or context".to_string(),
            )
        })
}

pub fn format_table_identifier(table_id: &TableId) -> String {
    format!(
        "{}.{}",
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
    )
}

pub fn format_table_identifier_opt(
    namespace: Option<&NamespaceId>,
    table_name: &TableName,
) -> String {
    match namespace {
        Some(ns) => format!("{}.{}", ns.as_str(), table_name.as_str()),
        None => table_name.as_str().to_string(),
    }
}

pub fn validate_table_name(table_name: &TableName) -> Result<(), KalamDbError> {
    kalamdb_commons::validation::validate_table_name(table_name.as_str())
        .map_err(|e| KalamDbError::InvalidOperation(e.to_string()))
}

pub fn validate_namespace_name(namespace: &NamespaceId) -> Result<(), KalamDbError> {
    kalamdb_commons::validation::validate_namespace_name(namespace.as_str())
        .map_err(|e| KalamDbError::InvalidOperation(e.to_string()))
}
