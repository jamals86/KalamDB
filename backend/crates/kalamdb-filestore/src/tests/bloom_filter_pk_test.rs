//! Bloom Filter for PRIMARY KEY columns test (FR-054, FR-055)
//!
//! Tests that Bloom filters are generated for PRIMARY KEY columns + _seq system column.
//! This enables efficient point query filtering (WHERE id = X) by skipping batch files
//! where the Bloom filter indicates "definitely not present".

use crate::parquet_writer::ParquetWriter;
use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};
use std::env;
use std::fs;
use std::sync::Arc;

/// Test that Bloom filters are generated for PRIMARY KEY column + _seq
#[test]
fn test_bloom_filter_for_primary_key_column() {
    let temp_dir = env::temp_dir().join("kalamdb_bloom_filter_pk_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // Schema with PRIMARY KEY column "id" + _seq system column
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),      // PRIMARY KEY
        Field::new("name", DataType::Utf8, true),       // Regular column
        Field::new("_seq", DataType::Int64, false),     // System column
    ]));

    // Create a batch with sample data
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1001, 1002, 1003, 1004, 1005])),
            Arc::new(StringArray::from(vec!["Alice", "Bob", "Charlie", "Diana", "Eve"])),
            Arc::new(Int64Array::from(vec![
                1000000, 2000000, 3000000, 4000000, 5000000,
            ])),
        ],
    )
    .unwrap();

    // Write Parquet file with Bloom filters on PRIMARY KEY ("id") and _seq
    let file_path = temp_dir.join("with_pk_bloom.parquet");
    let writer = ParquetWriter::new(file_path.to_str().unwrap());

    // Specify Bloom filter columns: PRIMARY KEY ("id") + _seq
    let bloom_filter_columns = vec!["id".to_string(), "_seq".to_string()];
    let result = writer.write_with_bloom_filter(schema, vec![batch], Some(bloom_filter_columns));
    assert!(result.is_ok());
    assert!(file_path.exists());

    // Verify Bloom filters exist in Parquet metadata
    let file = std::fs::File::open(&file_path).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    let metadata = reader.metadata();

    // Check that file has row groups
    assert!(metadata.num_row_groups() > 0);

    // Get first row group metadata
    let row_group = metadata.row_group(0);

    // Find column indexes for "id" and "_seq"
    let id_col_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "id")
        .expect("PRIMARY KEY column 'id' should exist");

    let seq_col_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "_seq")
        .expect("System column '_seq' should exist");

    // Verify "id" column has Bloom filter (FR-055: indexed columns)
    let id_col_metadata = row_group.column(id_col_idx);
    assert!(
        id_col_metadata.bloom_filter_offset().is_some(),
        "PRIMARY KEY column 'id' should have Bloom filter (FR-055)"
    );

    // Verify "_seq" column has Bloom filter (FR-054: default columns)
    let seq_col_metadata = row_group.column(seq_col_idx);
    assert!(
        seq_col_metadata.bloom_filter_offset().is_some(),
        "System column '_seq' should have Bloom filter (FR-054)"
    );

    // Verify "name" column does NOT have Bloom filter (not PRIMARY KEY)
    let name_col_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "name")
        .expect("Column 'name' should exist");

    let name_col_metadata = row_group.column(name_col_idx);
    assert!(
        name_col_metadata.bloom_filter_offset().is_none(),
        "Regular column 'name' should NOT have Bloom filter (not indexed)"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

/// Test that Bloom filters work with composite PRIMARY KEY (multiple columns)
#[test]
fn test_bloom_filter_for_composite_primary_key() {
    let temp_dir = env::temp_dir().join("kalamdb_bloom_filter_composite_pk_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // Schema with composite PRIMARY KEY (user_id, order_id) + _seq
    let schema = Arc::new(Schema::new(vec![
        Field::new("user_id", DataType::Int64, false),  // PRIMARY KEY part 1
        Field::new("order_id", DataType::Int64, false), // PRIMARY KEY part 2
        Field::new("amount", DataType::Int64, true),    // Regular column
        Field::new("_seq", DataType::Int64, false),     // System column
    ]));

    // Create batch with sample data
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![100, 100, 200, 200, 300])),
            Arc::new(Int64Array::from(vec![1, 2, 1, 2, 1])),
            Arc::new(Int64Array::from(vec![50, 75, 100, 125, 150])),
            Arc::new(Int64Array::from(vec![
                1000000, 2000000, 3000000, 4000000, 5000000,
            ])),
        ],
    )
    .unwrap();

    // Write Parquet file with Bloom filters on both PK columns + _seq
    let file_path = temp_dir.join("with_composite_pk_bloom.parquet");
    let writer = ParquetWriter::new(file_path.to_str().unwrap());

    // Specify Bloom filter columns: both PRIMARY KEY columns + _seq
    let bloom_filter_columns = vec![
        "user_id".to_string(),
        "order_id".to_string(),
        "_seq".to_string(),
    ];
    let result = writer.write_with_bloom_filter(schema, vec![batch], Some(bloom_filter_columns));
    assert!(result.is_ok());
    assert!(file_path.exists());

    // Verify Bloom filters exist for both PRIMARY KEY columns
    let file = std::fs::File::open(&file_path).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    let metadata = reader.metadata();
    let row_group = metadata.row_group(0);

    // Verify user_id has Bloom filter
    let user_id_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "user_id")
        .expect("user_id column should exist");
    assert!(
        row_group.column(user_id_idx).bloom_filter_offset().is_some(),
        "user_id (PRIMARY KEY part 1) should have Bloom filter"
    );

    // Verify order_id has Bloom filter
    let order_id_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "order_id")
        .expect("order_id column should exist");
    assert!(
        row_group.column(order_id_idx).bloom_filter_offset().is_some(),
        "order_id (PRIMARY KEY part 2) should have Bloom filter"
    );

    // Verify _seq has Bloom filter
    let seq_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "_seq")
        .expect("_seq column should exist");
    assert!(
        row_group.column(seq_idx).bloom_filter_offset().is_some(),
        "_seq (system column) should have Bloom filter"
    );

    // Verify amount does NOT have Bloom filter (not PRIMARY KEY)
    let amount_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "amount")
        .expect("amount column should exist");
    assert!(
        row_group.column(amount_idx).bloom_filter_offset().is_none(),
        "amount (regular column) should NOT have Bloom filter"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

/// Test that default behavior (None) only creates Bloom filter for _seq
#[test]
fn test_bloom_filter_default_behavior() {
    let temp_dir = env::temp_dir().join("kalamdb_bloom_filter_default_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // Schema with PRIMARY KEY + _seq
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("_seq", DataType::Int64, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])),
            Arc::new(Int64Array::from(vec![1000000, 2000000, 3000000])),
        ],
    )
    .unwrap();

    // Write with None (default behavior: only _seq gets Bloom filter)
    let file_path = temp_dir.join("default_bloom.parquet");
    let writer = ParquetWriter::new(file_path.to_str().unwrap());
    let result = writer.write_with_bloom_filter(schema, vec![batch], None);
    assert!(result.is_ok());

    // Verify only _seq has Bloom filter
    let file = std::fs::File::open(&file_path).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    let metadata = reader.metadata();
    let row_group = metadata.row_group(0);

    // _seq should have Bloom filter
    let seq_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "_seq")
        .unwrap();
    assert!(
        row_group.column(seq_idx).bloom_filter_offset().is_some(),
        "_seq should have Bloom filter by default"
    );

    // id should NOT have Bloom filter (default behavior doesn't include PRIMARY KEY)
    let id_idx = row_group
        .columns()
        .iter()
        .position(|col| col.column_path().string() == "id")
        .unwrap();
    assert!(
        row_group.column(id_idx).bloom_filter_offset().is_none(),
        "id should NOT have Bloom filter by default (must be explicitly specified)"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
