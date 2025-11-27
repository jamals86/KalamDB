// T052-T054: Version Resolution using Arrow Compute Kernels
//
// Selects MAX(_seq) per row_id with tie-breaker: FastStorage (priority=2) > Parquet (priority=1)

use super::arrow_json_conversion::arrow_value_to_scalar;
use crate::error::KalamDbError;
use datafusion::arrow::array::{
    Array, ArrayRef, BooleanArray, Int32Array, Int64Array, RecordBatch, StringArray, UInt64Array,
};
use datafusion::arrow::compute;
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::Row;
use kalamdb_tables::{SharedTableRow, UserTableRow};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub async fn resolve_latest_version(
    fast_batch: RecordBatch,
    long_batch: RecordBatch,
    schema: Arc<Schema>,
) -> Result<RecordBatch, KalamDbError> {
    // Handle empty batches - but still project to target schema for type conversions
    if fast_batch.num_rows() == 0 && long_batch.num_rows() == 0 {
        return create_empty_batch(&schema);
    }

    // If only one batch has data, still project it to ensure schema alignment
    if fast_batch.num_rows() == 0 {
        return project_to_target_schema(long_batch, schema);
    }
    if long_batch.num_rows() == 0 {
        return project_to_target_schema(fast_batch, schema);
    }

    let fast_with_priority = add_source_priority(fast_batch, 2)?;
    let long_with_priority = add_source_priority(long_batch, 1)?;

    let combined_schema = fast_with_priority.schema();
    let combined =
        compute::concat_batches(&combined_schema, &[fast_with_priority, long_with_priority])
            .map_err(|e| KalamDbError::Other(format!("concat: {}", e)))?;

    let row_id_idx = combined_schema
        .fields()
        .iter()
        .position(|f| f.name() == "row_id")
        .ok_or_else(|| KalamDbError::Other("Missing row_id".into()))?;
    let seq_idx = combined_schema
        .fields()
        .iter()
        .position(|f| f.name() == SystemColumnNames::SEQ)
        .ok_or_else(|| KalamDbError::Other("Missing _seq".into()))?;
    let priority_idx = combined_schema
        .fields()
        .iter()
        .position(|f| f.name() == "source_priority")
        .ok_or_else(|| KalamDbError::Other("Missing source_priority".into()))?;

    let row_id_array = combined
        .column(row_id_idx)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| KalamDbError::Other("row_id not StringArray".into()))?;
    let seq_array = combined
        .column(seq_idx)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| KalamDbError::Other("_seq not Int64Array".into()))?;
    let priority_array = combined
        .column(priority_idx)
        .as_any()
        .downcast_ref::<Int32Array>()
        .ok_or_else(|| KalamDbError::Other("source_priority not Int32Array".into()))?;

    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for i in 0..combined.num_rows() {
        groups
            .entry(row_id_array.value(i).to_string())
            .or_default()
            .push(i);
    }

    let mut keep_indices = Vec::with_capacity(groups.len());
    for indices in groups.values() {
        if indices.len() == 1 {
            keep_indices.push(indices[0]);
            continue;
        }
        let mut best_idx = indices[0];
        let mut best_seq = seq_array.value(best_idx);
        let mut best_priority = priority_array.value(best_idx);
        for &idx in &indices[1..] {
            let seq = seq_array.value(idx);
            let priority = priority_array.value(idx);
            if seq > best_seq || (seq == best_seq && priority > best_priority) {
                best_idx = idx;
                best_seq = seq;
                best_priority = priority;
            }
        }
        keep_indices.push(best_idx);
    }
    keep_indices.sort_unstable();

    let indices_array = Arc::new(UInt64Array::from(
        keep_indices.iter().map(|&i| i as u64).collect::<Vec<_>>(),
    ));
    let result_columns: Result<Vec<ArrayRef>, _> = combined
        .columns()
        .iter()
        .map(|col| {
            compute::take(col.as_ref(), indices_array.as_ref(), None)
                .map_err(|e| KalamDbError::Other(format!("take: {}", e)))
        })
        .collect();
    let result_batch = RecordBatch::try_new(combined_schema.clone(), result_columns?)
        .map_err(|e| KalamDbError::Other(format!("create batch: {}", e)))?;

    // Project to target schema with type conversions
    project_to_target_schema(result_batch, schema)
}

/// Project RecordBatch to target schema with type conversions
///
/// Handles type conversions needed after version resolution:
/// - _deleted: Should already be Boolean, but verify and convert if needed
fn project_to_target_schema(
    batch: RecordBatch,
    schema: Arc<Schema>,
) -> Result<RecordBatch, KalamDbError> {
    let mut final_columns: Vec<ArrayRef> = Vec::new();

    for field in schema.fields().iter() {
        let col = batch.column_by_name(field.name()).ok_or_else(|| {
            KalamDbError::Other(format!("Missing column {} in batch", field.name()))
        })?;

        // Ensure _deleted remains Boolean (should already be, but verify)
        if field.name() == SystemColumnNames::DELETED && field.data_type() == &DataType::Boolean {
            // _deleted should already be BooleanArray, but double-check
            if col.as_any().downcast_ref::<BooleanArray>().is_none() {
                log::warn!(
                    "_deleted column is not BooleanArray in projection, attempting conversion"
                );
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

    RecordBatch::try_new(schema, final_columns)
        .map_err(|e| KalamDbError::Other(format!("project_to_target_schema: {}", e)))
}

fn add_source_priority(batch: RecordBatch, priority: u8) -> Result<RecordBatch, KalamDbError> {
    let num_rows = batch.num_rows();
    let priority_array: ArrayRef = Arc::new(Int32Array::from(vec![priority as i32; num_rows]));
    let mut fields = batch.schema().fields().to_vec();
    fields.push(Arc::new(Field::new(
        "source_priority",
        DataType::Int32,
        false,
    )));
    let new_schema = Arc::new(Schema::new(fields));
    let mut columns: Vec<ArrayRef> = batch.columns().to_vec();
    columns.push(priority_array);
    RecordBatch::try_new(new_schema, columns)
        .map_err(|e| KalamDbError::Other(format!("add_source_priority: {}", e)))
}

fn create_empty_batch(schema: &Arc<Schema>) -> Result<RecordBatch, KalamDbError> {
    use datafusion::arrow::array::Int64Array;
    let empty_columns: Vec<ArrayRef> = schema
        .fields()
        .iter()
        .map(|field| match field.data_type() {
            DataType::Utf8 => Arc::new(StringArray::from(Vec::<&str>::new())) as ArrayRef,
            DataType::Int64 => Arc::new(Int64Array::from(Vec::<i64>::new())) as ArrayRef,
            DataType::Boolean => Arc::new(BooleanArray::from(Vec::<bool>::new())) as ArrayRef,
            DataType::Timestamp(_, _) => Arc::new(Int64Array::from(Vec::<i64>::new())) as ArrayRef,
            _ => Arc::new(StringArray::from(Vec::<&str>::new())) as ArrayRef,
        })
        .collect();
    RecordBatch::try_new(schema.clone(), empty_columns)
        .map_err(|e| KalamDbError::Other(format!("empty_batch: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_empty() {
        let s = Arc::new(Schema::new(vec![
            Field::new("row_id", DataType::Utf8, false),
            Field::new(SystemColumnNames::SEQ, DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));
        let e = RecordBatch::try_new(
            s.clone(),
            vec![
                Arc::new(StringArray::from(Vec::<&str>::new())),
                Arc::new(Int64Array::from(Vec::<i64>::new())),
                Arc::new(StringArray::from(Vec::<&str>::new())),
            ],
        )
        .unwrap();
        assert_eq!(
            resolve_latest_version(e.clone(), e, s)
                .await
                .unwrap()
                .num_rows(),
            0
        );
    }
    #[tokio::test]
    async fn test_max_seq() {
        let s = Arc::new(Schema::new(vec![
            Field::new("row_id", DataType::Utf8, false),
            Field::new(SystemColumnNames::SEQ, DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));
        let f = RecordBatch::try_new(
            s.clone(),
            vec![
                Arc::new(StringArray::from(vec!["1"])),
                Arc::new(Int64Array::from(vec![2])),
                Arc::new(StringArray::from(vec!["Alice_v2"])),
            ],
        )
        .unwrap();
        let l = RecordBatch::try_new(
            s.clone(),
            vec![
                Arc::new(StringArray::from(vec!["1"])),
                Arc::new(Int64Array::from(vec![1])),
                Arc::new(StringArray::from(vec!["Alice_v1"])),
            ],
        )
        .unwrap();
        let r = resolve_latest_version(f, l, s).await.unwrap();
        assert_eq!(r.num_rows(), 1);
        assert_eq!(
            r.column(2)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "Alice_v2"
        );
    }
    #[tokio::test]
    async fn test_tie_breaker() {
        let s = Arc::new(Schema::new(vec![
            Field::new("row_id", DataType::Utf8, false),
            Field::new(SystemColumnNames::SEQ, DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));
        let f = RecordBatch::try_new(
            s.clone(),
            vec![
                Arc::new(StringArray::from(vec!["1"])),
                Arc::new(Int64Array::from(vec![1])),
                Arc::new(StringArray::from(vec!["Fast"])),
            ],
        )
        .unwrap();
        let l = RecordBatch::try_new(
            s.clone(),
            vec![
                Arc::new(StringArray::from(vec!["1"])),
                Arc::new(Int64Array::from(vec![1])),
                Arc::new(StringArray::from(vec!["Parquet"])),
            ],
        )
        .unwrap();
        let r = resolve_latest_version(f, l, s).await.unwrap();
        assert_eq!(r.num_rows(), 1);
        assert_eq!(
            r.column(2)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "Fast"
        );
    }
}

/// Generic helper for UPDATE/DELETE operations that need to scan hot+cold storage with version resolution
///
/// This is a generic helper for UPDATE/DELETE operations that need to:
/// 1. Scan both RocksDB (fast storage) and Parquet (flushed storage)
/// 2. Apply version resolution (latest _seq wins)
/// 3. Filter out deleted records (_deleted = true)
/// 4. Convert Arrow RecordBatch back to row structures
///
/// **Phase 3, US1, T061-T068**: Support UPDATE/DELETE on flushed records
/// **Phase 13.6**: Moved from base_table_provider.rs during cleanup
///
/// # Type Parameters
/// * `K` - Storage key type
/// * `V` - Row value type
/// * `F` - Function to scan RocksDB, returns RecordBatch
/// * `G` - Function to scan Parquet, returns RecordBatch
/// * `H` - Function to convert RecordBatch row to (key, value) pair
///
/// # Arguments
/// * `schema` - Arrow schema with system columns
/// * `scan_rocksdb` - Async function to scan RocksDB
/// * `scan_parquet` - Async function to scan Parquet files
/// * `row_converter` - Function to convert Arrow row to (key, value)
///
/// # Returns
/// Vector of (key, value) pairs representing latest non-deleted records
pub async fn scan_with_version_resolution_to_kvs<K, V, F, G, H>(
    schema: Arc<datafusion::arrow::datatypes::Schema>,
    scan_rocksdb: F,
    scan_parquet: G,
    row_converter: H,
) -> Result<Vec<(K, V)>, datafusion::error::DataFusionError>
where
    K: Clone,
    F: std::future::Future<
        Output = Result<
            datafusion::arrow::record_batch::RecordBatch,
            datafusion::error::DataFusionError,
        >,
    >,
    G: std::future::Future<
        Output = Result<
            datafusion::arrow::record_batch::RecordBatch,
            datafusion::error::DataFusionError,
        >,
    >,
    H: Fn(
        &datafusion::arrow::record_batch::RecordBatch,
        usize,
    ) -> Result<(K, V), datafusion::error::DataFusionError>,
{
    use datafusion::arrow::array::AsArray;
    use datafusion::arrow::compute;

    // Step 1: Scan RocksDB (fast storage)
    let rocksdb_batch = scan_rocksdb.await?;

    // Step 2: Scan Parquet files (flushed storage)
    let parquet_batch = scan_parquet.await?;

    // Step 3: Apply version resolution (latest _seq wins)
    let resolved_batch = resolve_latest_version(rocksdb_batch, parquet_batch, schema.clone())
        .await
        .map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!(
                "Version resolution failed: {}",
                e
            ))
        })?;

    // Step 4: Filter out deleted records (_deleted = true)
    let filtered_batch = {
        let deleted_col = resolved_batch.column_by_name("_deleted").ok_or_else(|| {
            datafusion::error::DataFusionError::Execution("Missing _deleted column".to_string())
        })?;
        let deleted_array = deleted_col.as_boolean();

        // Create filter: NOT deleted (keep rows where _deleted = false)
        let filter_array = compute::not(deleted_array).map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!(
                "Failed to compute NOT filter: {}",
                e
            ))
        })?;

        compute::filter_record_batch(&resolved_batch, &filter_array).map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!(
                "Failed to filter deleted records: {}",
                e
            ))
        })?
    };

    // Step 5: Convert Arrow RecordBatch to Vec<(K, V)>
    let num_rows = filtered_batch.num_rows();
    let mut results = Vec::with_capacity(num_rows);

    for row_idx in 0..num_rows {
        let (key, value) = row_converter(&filtered_batch, row_idx)?;
        results.push((key, value));
    }

    Ok(results)
}

/// Parsed representation of a Parquet row used for version resolution merging
#[derive(Debug, Clone)]
pub struct ParquetRowData {
    pub seq_id: SeqId,
    pub deleted: bool,
    pub fields: Row,
}

/// Convert Parquet RecordBatch rows into SeqId + JSON field maps
pub fn parquet_batch_to_rows(batch: &RecordBatch) -> Result<Vec<ParquetRowData>, KalamDbError> {
    use datafusion::arrow::array::{Array, BooleanArray, Int64Array};

    if batch.num_rows() == 0 {
        return Ok(Vec::new());
    }

    let schema = batch.schema();
    let seq_idx = schema
        .fields()
        .iter()
        .position(|f| f.name() == "_seq")
        .ok_or_else(|| KalamDbError::Other("Missing _seq column in Parquet batch".to_string()))?;
    let deleted_idx = schema.fields().iter().position(|f| f.name() == "_deleted");

    let seq_array = batch
        .column(seq_idx)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| KalamDbError::Other("_seq column is not Int64Array".to_string()))?;
    let deleted_array =
        deleted_idx.and_then(|idx| batch.column(idx).as_any().downcast_ref::<BooleanArray>());

    let mut rows = Vec::with_capacity(batch.num_rows());
    for row_idx in 0..batch.num_rows() {
        let seq_val = seq_array.value(row_idx);
        let seq_id = SeqId::from_i64(seq_val);
        let deleted = deleted_array
            .and_then(|arr| {
                if !arr.is_null(row_idx) {
                    Some(arr.value(row_idx))
                } else {
                    None
                }
            })
            .unwrap_or(false);

        let mut values = BTreeMap::new();
        for (col_idx, field) in schema.fields().iter().enumerate() {
            let col_name = field.name();
            if col_name == "_seq" || col_name == "_deleted" {
                continue;
            }

            let array = batch.column(col_idx);
            match arrow_value_to_scalar(array.as_ref(), row_idx) {
                Ok(val) => {
                    values.insert(col_name.clone(), val);
                }
                Err(e) => {
                    log::warn!(
                        "Failed to convert column {} for row {}: {}",
                        col_name,
                        row_idx,
                        e
                    );
                }
            }
        }

        rows.push(ParquetRowData {
            seq_id,
            deleted,
            fields: Row::new(values),
        });
    }

    Ok(rows)
}

/// Trait covering the minimal information needed for merging versioned rows
pub trait VersionedRow {
    fn seq_id(&self) -> SeqId;
    fn deleted(&self) -> bool;
    fn pk_value(&self, pk_name: &str) -> Option<String>;
}

impl VersionedRow for SharedTableRow {
    fn seq_id(&self) -> SeqId {
        self._seq
    }

    fn deleted(&self) -> bool {
        self._deleted
    }

    fn pk_value(&self, pk_name: &str) -> Option<String> {
        self.fields.get(pk_name).and_then(|val| {
            if val.is_null() {
                None
            } else {
                match val {
                    ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => Some(s.clone()),
                    _ => Some(val.to_string()),
                }
            }
        })
    }
}

impl VersionedRow for UserTableRow {
    fn seq_id(&self) -> SeqId {
        self._seq
    }

    fn deleted(&self) -> bool {
        self._deleted
    }

    fn pk_value(&self, pk_name: &str) -> Option<String> {
        self.fields.get(pk_name).and_then(|val| {
            if val.is_null() {
                None
            } else {
                match val {
                    ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => Some(s.clone()),
                    _ => Some(val.to_string()),
                }
            }
        })
    }
}

/// Merge hot (RocksDB) and cold (Parquet) rows keeping latest version per PK
pub fn merge_versioned_rows<K, R, I, J>(
    pk_name: &str,
    hot_rows: I,
    cold_rows: J,
    keep_deleted: bool,
) -> Vec<(K, R)>
where
    I: IntoIterator<Item = (K, R)>,
    J: IntoIterator<Item = (K, R)>,
    K: Clone,
    R: VersionedRow,
{
    use std::collections::hash_map::Entry;

    let mut best: HashMap<String, (K, R)> = HashMap::new();

    for (key, row) in hot_rows.into_iter().chain(cold_rows) {
        let pk_key = row
            .pk_value(pk_name)
            .filter(|val| !val.is_empty())
            .unwrap_or_else(|| format!("_seq:{}", row.seq_id().as_i64()));

        match best.entry(pk_key) {
            Entry::Occupied(mut entry) => {
                if row.seq_id() > entry.get().1.seq_id() {
                    entry.insert((key, row));
                }
            }
            Entry::Vacant(entry) => {
                entry.insert((key, row));
            }
        }
    }

    best.into_values()
        .filter(|(_, row)| keep_deleted || !row.deleted())
        .collect()
}
