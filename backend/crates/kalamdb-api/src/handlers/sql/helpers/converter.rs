//! Arrow to JSON conversion helpers

use arrow::record_batch::RecordBatch;
use kalamdb_commons::conversions::{
    read_kalam_data_type_metadata, KALAM_DATA_TYPE_METADATA_KEY,
};
use kalamdb_commons::models::datatypes::{FromArrowType, KalamDataType};
use kalamdb_commons::models::Role;
use kalamdb_commons::schemas::SchemaField;
use kalamdb_core::providers::arrow_json_conversion::record_batch_to_json_arrays;
use std::collections::HashMap;

use super::super::models::QueryResult;

/// Convert Arrow RecordBatches to QueryResult
pub fn record_batch_to_query_result(
    batches: Vec<RecordBatch>,
    schema: Option<arrow::datatypes::SchemaRef>,
    user_role: Option<Role>,
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    // Get schema from first batch, or from explicitly provided schema for empty results
    let arrow_schema = if !batches.is_empty() {
        batches[0].schema()
    } else if let Some(s) = schema {
        s
    } else {
        // No batches and no schema - truly empty result
        return Ok(QueryResult::with_message("Query executed successfully".to_string()));
    };

    // Build SchemaField with KalamDataType from Arrow schema
    let schema_fields: Vec<SchemaField> = arrow_schema
        .fields()
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let kalam_type = if let Some(metadata_type) = read_kalam_data_type_metadata(field) {
                metadata_type
            } else {
                if field.metadata().contains_key(KALAM_DATA_TYPE_METADATA_KEY) {
                    log::warn!(
                        "Invalid '{}' metadata for column '{}'; falling back to Arrow type inference ({:?})",
                        KALAM_DATA_TYPE_METADATA_KEY,
                        field.name(),
                        field.data_type()
                    );
                }

                match KalamDataType::from_arrow_type(field.data_type()) {
                    Ok(inferred_type) => inferred_type,
                    Err(err) => {
                        log::warn!(
                            "Unsupported Arrow type {:?} for column '{}': {}. Defaulting schema type to Text",
                            field.data_type(),
                            field.name(),
                            err
                        );
                        KalamDataType::Text
                    },
                }
            };

            SchemaField::new(field.name().clone(), kalam_type, index)
        })
        .collect();

    // Build column name to index mapping for sensitive column masking
    let column_indices: HashMap<String, usize> = arrow_schema
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name().to_lowercase(), i))
        .collect();

    let mut rows = Vec::new();
    for batch in &batches {
        let batch_rows = record_batch_to_json_arrays(batch)
            .map_err(|e| format!("Failed to convert batch to JSON: {}", e))?;
        rows.extend(batch_rows);
    }

    // Mask sensitive columns for non-admin users
    if !is_admin_role(user_role) {
        mask_sensitive_column_array(&mut rows, &column_indices, "credentials");
        mask_sensitive_column_array(&mut rows, &column_indices, "password_hash");
    }

    let result = QueryResult::with_rows_and_schema(rows, schema_fields);
    Ok(result)
}

/// Mask a sensitive column with "***" (for array-based rows)
fn mask_sensitive_column_array(
    rows: &mut [Vec<serde_json::Value>],
    column_indices: &HashMap<String, usize>,
    target_column: &str,
) {
    if let Some(&col_idx) = column_indices.get(&target_column.to_lowercase()) {
        for row in rows.iter_mut() {
            if let Some(value) = row.get_mut(col_idx) {
                if !value.is_null() {
                    *value = serde_json::Value::String("***".to_string());
                }
            }
        }
    }
}

/// Check if user has admin privileges for viewing sensitive data.
fn is_admin_role(role: Option<Role>) -> bool {
    matches!(role, Some(Role::Dba) | Some(Role::System))
}
