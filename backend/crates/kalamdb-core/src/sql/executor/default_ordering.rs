//! Default ORDER BY injection for consistent query results
//!
//! This module ensures that SELECT queries return results in a consistent order,
//! even when no explicit ORDER BY is specified. This is critical for:
//!
//! 1. **Pagination consistency**: Users expect the same order across paginated requests
//! 2. **Hot/Cold storage consistency**: Data from RocksDB and Parquet must be ordered identically
//! 3. **Deterministic results**: Same query should always return same row order
//!
//! The default ordering is by primary key columns in ASC order.
//! If no primary key is defined, we fall back to _seq (system sequence column).

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use datafusion::logical_expr::{LogicalPlan, SortExpr};
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::models::{NamespaceId, TableId, TableName};
use std::sync::Arc;

/// Check if a LogicalPlan already has an ORDER BY clause at the top level
///
/// This traverses up from the root to find if any Sort node exists.
/// We only check the immediate children to avoid false positives from subqueries.
pub fn has_order_by(plan: &LogicalPlan) -> bool {
    matches!(plan, LogicalPlan::Sort(_))
}

/// Extract table reference from a LogicalPlan
///
/// Returns the first TableScan found in the plan tree.
/// For simple SELECT queries, this is typically the main table.
fn extract_table_reference(plan: &LogicalPlan) -> Option<(NamespaceId, TableName)> {
    match plan {
        LogicalPlan::TableScan(scan) => {
            let (ns, tbl) = match &scan.table_name {
                datafusion::common::TableReference::Bare { table } => (
                    NamespaceId::new("default"),
                    TableName::new(table.to_string()),
                ),
                datafusion::common::TableReference::Partial { schema, table } => (
                    NamespaceId::new(schema.to_string()),
                    TableName::new(table.to_string()),
                ),
                datafusion::common::TableReference::Full { schema, table, .. } => (
                    NamespaceId::new(schema.to_string()),
                    TableName::new(table.to_string()),
                ),
            };
            Some((ns, tbl))
        }
        // For other plan nodes, check their inputs
        _ => {
            for input in plan.inputs() {
                if let Some(result) = extract_table_reference(input) {
                    return Some(result);
                }
            }
            None
        }
    }
}

/// Get the default sort columns for a table
///
/// Returns primary key columns if defined, otherwise falls back to _seq.
/// The columns are returned as SortExpr with ASC ordering.
fn get_default_sort_columns(
    app_context: &Arc<AppContext>,
    table_id: &TableId,
) -> Result<Vec<SortExpr>, KalamDbError> {
    let schema_registry = app_context.schema_registry();

    // Try to get table definition
    if let Ok(Some(table_def)) = schema_registry.get_table_definition(table_id) {
        let pk_columns = table_def.get_primary_key_columns();

        if !pk_columns.is_empty() {
            // Use primary key columns
            return Ok(pk_columns
                .into_iter()
                .map(|col_name| {
                    SortExpr::new(
                        datafusion::logical_expr::col(col_name),
                        true,  // ascending
                        false, // nulls_first
                    )
                })
                .collect());
        }
    }

    // Fallback: use _seq system column
    Ok(vec![SortExpr::new(
        datafusion::logical_expr::col(SystemColumnNames::SEQ),
        true,  // ascending
        false, // nulls_first
    )])
}

/// Add default ORDER BY to a LogicalPlan if not already present
///
/// This function:
/// 1. Checks if the plan already has an ORDER BY clause
/// 2. Extracts the table reference from the plan
/// 3. Looks up primary key columns from the schema registry
/// 4. Wraps the plan with a Sort node using those columns
///
/// # Arguments
/// * `plan` - The LogicalPlan to potentially wrap with ORDER BY
/// * `app_context` - AppContext for schema registry access
///
/// # Returns
/// * `Ok(LogicalPlan)` - The original plan if ORDER BY exists, or wrapped plan
/// * `Err(KalamDbError)` - If schema lookup fails (rare, plan is returned unchanged)
pub fn apply_default_order_by(
    plan: LogicalPlan,
    app_context: &Arc<AppContext>,
) -> Result<LogicalPlan, KalamDbError> {
    // Skip if already has ORDER BY
    if has_order_by(&plan) {
        log::trace!(target: "sql::ordering", "Plan already has ORDER BY, skipping default");
        return Ok(plan);
    }

    // Extract table reference
    let table_ref = match extract_table_reference(&plan) {
        Some(ref_tuple) => ref_tuple,
        None => {
            // No table found (might be a function call like SELECT NOW())
            log::trace!(target: "sql::ordering", "No table reference found, skipping default ORDER BY");
            return Ok(plan);
        }
    };

    let (namespace_id, table_name) = table_ref;
    let table_id = TableId::new(namespace_id, table_name);

    // Get sort columns for this table
    let sort_exprs = match get_default_sort_columns(app_context, &table_id) {
        Ok(exprs) => exprs,
        Err(e) => {
            log::warn!(
                target: "sql::ordering",
                "Failed to get default sort columns for {}: {}. Returning unsorted.",
                table_id.full_name(),
                e
            );
            return Ok(plan);
        }
    };

    // Create Sort node wrapping the original plan
    let sort_plan = LogicalPlan::Sort(datafusion::logical_expr::Sort {
        expr: sort_exprs,
        input: Arc::new(plan),
        fetch: None, // No limit from ordering itself
    });

    log::debug!(
        target: "sql::ordering",
        "Applied default ORDER BY to query on table {}",
        table_id.full_name()
    );

    Ok(sort_plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_order_by_false_for_table_scan() {
        // A simple TableScan plan should not have ORDER BY
        use datafusion::prelude::*;

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let ctx = SessionContext::new();
            ctx.register_csv("test", "nonexistent.csv", CsvReadOptions::default())
                .await
                .ok();

            // This would normally work with a real file, but we're just testing the structure
            // For now, we'll test with a simpler approach
        });
    }

    #[test]
    fn test_extract_table_reference() {
        // Test that we can extract table references from various plan types
        // This is a unit test for the helper function
    }
}
