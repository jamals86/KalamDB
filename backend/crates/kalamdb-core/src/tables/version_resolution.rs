// T052-T054: Version Resolution using Arrow Compute Kernels
//
// Selects MAX(_updated) per row_id with tie-breaker: FastStorage (priority=2) > Parquet (priority=1)

use datafusion::arrow::array::{Array, ArrayRef, BooleanArray, Int32Array, RecordBatch, StringArray, UInt64Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::compute;
use std::collections::HashMap;
use std::sync::Arc;
use crate::error::KalamDbError;


pub async fn resolve_latest_version(
    fast_batch: RecordBatch,
    long_batch: RecordBatch,
    schema: Arc<Schema>,
) -> Result<RecordBatch, KalamDbError> {
    // Handle empty batches - but still project to target schema for type conversions
    if fast_batch.num_rows() == 0 && long_batch.num_rows() == 0 {
        return create_empty_batch(&schema);
    }
    
    // If only one batch has data, still project it to ensure type conversions (e.g., String→Timestamp for _updated)
    if fast_batch.num_rows() == 0 {
        return project_to_target_schema(long_batch, schema);
    }
    if long_batch.num_rows() == 0 {
        return project_to_target_schema(fast_batch, schema);
    }

    let fast_with_priority = add_source_priority(fast_batch, 2)?;
    let long_with_priority = add_source_priority(long_batch, 1)?;

    let combined_schema = fast_with_priority.schema();
    let combined = compute::concat_batches(&combined_schema, &[fast_with_priority, long_with_priority])
        .map_err(|e| KalamDbError::Other(format!("concat: {}", e)))?;

    let row_id_idx = combined_schema.fields().iter().position(|f| f.name() == "row_id")
        .ok_or_else(|| KalamDbError::Other("Missing row_id".into()))?;
    let updated_idx = combined_schema.fields().iter().position(|f| f.name() == "_updated")
        .ok_or_else(|| KalamDbError::Other("Missing _updated".into()))?;
    let priority_idx = combined_schema.fields().iter().position(|f| f.name() == "source_priority")
        .ok_or_else(|| KalamDbError::Other("Missing source_priority".into()))?;

    let row_id_array = combined.column(row_id_idx).as_any().downcast_ref::<StringArray>()
        .ok_or_else(|| KalamDbError::Other("row_id not StringArray".into()))?;
    let updated_array = combined.column(updated_idx).as_any().downcast_ref::<StringArray>()
        .ok_or_else(|| KalamDbError::Other("_updated not StringArray".into()))?;
    let priority_array = combined.column(priority_idx).as_any().downcast_ref::<Int32Array>()
        .ok_or_else(|| KalamDbError::Other("source_priority not Int32Array".into()))?;

    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for i in 0..combined.num_rows() {
        groups.entry(row_id_array.value(i).to_string()).or_default().push(i);
    }

    let mut keep_indices = Vec::with_capacity(groups.len());
    for indices in groups.values() {
        if indices.len() == 1 {
            keep_indices.push(indices[0]);
            continue;
        }
        let mut best_idx = indices[0];
        let mut best_updated = updated_array.value(best_idx);
        let mut best_priority = priority_array.value(best_idx);
        for &idx in &indices[1..] {
            let updated = updated_array.value(idx);
            let priority = priority_array.value(idx);
            if updated > best_updated || (updated == best_updated && priority > best_priority) {
                best_idx = idx;
                best_updated = updated;
                best_priority = priority;
            }
        }
        keep_indices.push(best_idx);
    }
    keep_indices.sort_unstable();

    let indices_array = Arc::new(UInt64Array::from(keep_indices.iter().map(|&i| i as u64).collect::<Vec<_>>()));
    let result_columns: Result<Vec<ArrayRef>, _> = combined.columns().iter()
        .map(|col| compute::take(col.as_ref(), indices_array.as_ref(), None)
            .map_err(|e| KalamDbError::Other(format!("take: {}", e))))
        .collect();
    let result_batch = RecordBatch::try_new(combined_schema.clone(), result_columns?)
        .map_err(|e| KalamDbError::Other(format!("create batch: {}", e)))?;

    // Project to target schema with type conversions
    project_to_target_schema(result_batch, schema)
}

/// Project RecordBatch to target schema with type conversions
///
/// Handles type conversions needed after version resolution:
/// - _updated: String (RFC3339) → Timestamp(Millisecond) if schema expects Timestamp
/// - _deleted: Should already be Boolean, but verify and convert if needed
fn project_to_target_schema(batch: RecordBatch, schema: Arc<Schema>) -> Result<RecordBatch, KalamDbError> {
    let mut final_columns: Vec<ArrayRef> = Vec::new();
    
    for field in schema.fields().iter() {
        let col = batch.column_by_name(field.name())
            .ok_or_else(|| KalamDbError::Other(format!("Missing column {} in batch", field.name())))?;
        
        // Convert _updated from String back to Timestamp(Millisecond) if needed
        if field.name() == "_updated" && field.data_type() == &DataType::Timestamp(datafusion::arrow::datatypes::TimeUnit::Millisecond, None) {
            if let Some(string_array) = col.as_any().downcast_ref::<StringArray>() {
                // Convert RFC3339 strings back to millisecond timestamps
                use chrono::DateTime;
                let millis: Vec<i64> = (0..string_array.len())
                    .map(|i| {
                        DateTime::parse_from_rfc3339(string_array.value(i))
                            .map(|dt| dt.timestamp_millis())
                            .unwrap_or(0)
                    })
                    .collect();
                final_columns.push(Arc::new(datafusion::arrow::array::TimestampMillisecondArray::from(millis)) as ArrayRef);
                continue;
            }
        }
        
        // Ensure _deleted remains Boolean (should already be, but verify)
        if field.name() == "_deleted" && field.data_type() == &DataType::Boolean {
            // _deleted should already be BooleanArray, but double-check
            if col.as_any().downcast_ref::<BooleanArray>().is_none() {
                log::warn!("_deleted column is not BooleanArray in projection, attempting conversion");
                // Fallback: try to convert from other types if needed
                if let Some(string_array) = col.as_any().downcast_ref::<StringArray>() {
                    let bools: Vec<bool> = (0..string_array.len())
                        .map(|i| string_array.value(i) == "true")
                        .collect();
                    final_columns.push(Arc::new(BooleanArray::from(bools)) as ArrayRef);
                    continue;
                }
            }
        }
        
        final_columns.push(col.clone());
    }
    
    RecordBatch::try_new(schema, final_columns).map_err(|e| KalamDbError::Other(format!("project_to_target_schema: {}", e)))
}

fn add_source_priority(batch: RecordBatch, priority: u8) -> Result<RecordBatch, KalamDbError> {
    let num_rows = batch.num_rows();
    let priority_array: ArrayRef = Arc::new(Int32Array::from(vec![priority as i32; num_rows]));
    let mut fields = batch.schema().fields().to_vec();
    fields.push(Arc::new(Field::new("source_priority", DataType::Int32, false)));
    let new_schema = Arc::new(Schema::new(fields));
    let mut columns: Vec<ArrayRef> = batch.columns().to_vec();
    columns.push(priority_array);
    RecordBatch::try_new(new_schema, columns).map_err(|e| KalamDbError::Other(format!("add_source_priority: {}", e)))
}

fn create_empty_batch(schema: &Arc<Schema>) -> Result<RecordBatch, KalamDbError> {
    use datafusion::arrow::array::Int64Array;
    let empty_columns: Vec<ArrayRef> = schema.fields().iter()
        .map(|field| match field.data_type() {
            DataType::Utf8 => Arc::new(StringArray::from(Vec::<&str>::new())) as ArrayRef,
            DataType::Int64 => Arc::new(Int64Array::from(Vec::<i64>::new())) as ArrayRef,
            DataType::Boolean => Arc::new(BooleanArray::from(Vec::<bool>::new())) as ArrayRef,
            DataType::Timestamp(_, _) => Arc::new(Int64Array::from(Vec::<i64>::new())) as ArrayRef,
            _ => Arc::new(StringArray::from(Vec::<&str>::new())) as ArrayRef,
        })
        .collect();
    RecordBatch::try_new(schema.clone(), empty_columns).map_err(|e| KalamDbError::Other(format!("empty_batch: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_empty() {
        let s = Arc::new(Schema::new(vec![Field::new("row_id", DataType::Utf8, false), Field::new("_updated", DataType::Utf8, false), Field::new("name", DataType::Utf8, true)]));
        let e = RecordBatch::try_new(s.clone(), vec![Arc::new(StringArray::from(Vec::<&str>::new())), Arc::new(StringArray::from(Vec::<&str>::new())), Arc::new(StringArray::from(Vec::<&str>::new()))]).unwrap();
        assert_eq!(resolve_latest_version(e.clone(), e, s).await.unwrap().num_rows(), 0);
    }
    #[tokio::test]
    async fn test_max_updated() {
        let s = Arc::new(Schema::new(vec![Field::new("row_id", DataType::Utf8, false), Field::new("_updated", DataType::Utf8, false), Field::new("name", DataType::Utf8, true)]));
        let f = RecordBatch::try_new(s.clone(), vec![Arc::new(StringArray::from(vec!["1"])), Arc::new(StringArray::from(vec!["2025-01-15T10:01:00Z"])), Arc::new(StringArray::from(vec!["Alice_v2"]))]).unwrap();
        let l = RecordBatch::try_new(s.clone(), vec![Arc::new(StringArray::from(vec!["1"])), Arc::new(StringArray::from(vec!["2025-01-15T10:00:00Z"])), Arc::new(StringArray::from(vec!["Alice_v1"]))]).unwrap();
        let r = resolve_latest_version(f, l, s).await.unwrap();
        assert_eq!(r.num_rows(), 1);
        assert_eq!(r.column(2).as_any().downcast_ref::<StringArray>().unwrap().value(0), "Alice_v2");
    }
    #[tokio::test]
    async fn test_tie_breaker() {
        let s = Arc::new(Schema::new(vec![Field::new("row_id", DataType::Utf8, false), Field::new("_updated", DataType::Utf8, false), Field::new("name", DataType::Utf8, true)]));
        let f = RecordBatch::try_new(s.clone(), vec![Arc::new(StringArray::from(vec!["1"])), Arc::new(StringArray::from(vec!["2025-01-15T10:00:00Z"])), Arc::new(StringArray::from(vec!["Fast"]))]).unwrap();
        let l = RecordBatch::try_new(s.clone(), vec![Arc::new(StringArray::from(vec!["1"])), Arc::new(StringArray::from(vec!["2025-01-15T10:00:00Z"])), Arc::new(StringArray::from(vec!["Parquet"]))]).unwrap();
        let r = resolve_latest_version(f, l, s).await.unwrap();
        assert_eq!(r.num_rows(), 1);
        assert_eq!(r.column(2).as_any().downcast_ref::<StringArray>().unwrap().value(0), "Fast");
    }
}
