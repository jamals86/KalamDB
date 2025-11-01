//! End-to-end test: ensure all supported datatypes flush to Parquet and values are preserved

use std::fs;
use std::path::Path;
use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use arrow::array::Array; // for is_valid()
use base64::{engine::general_purpose, Engine as _};
use datafusion::catalog::memory::MemorySchemaProvider;
use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition, TableType};
use kalamdb_commons::models::types::KalamDataType;
use kalamdb_commons::{NamespaceId, TableId, TableName};
use kalamdb_core::system_table_registration::register_system_tables;
use kalamdb_core::tables::user_tables::user_table_flush::UserTableFlushJob;
use kalamdb_core::tables::user_tables::user_table_store::{new_user_table_store, UserTableRow, UserTableRowId};
use kalamdb_store::{EntityStoreV2, RocksDBBackend};
use rocksdb::DB;
use tempfile::TempDir;

#[tokio::test]
async fn test_datatypes_preservation_values() {
    // Setup temp RocksDB and output dir
    let temp = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(DB::open_default(temp.path().join("rocksdb").to_str().unwrap()).expect("Failed to create RocksDB"));
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    // Register system tables (schema store not strictly required here, but keep consistent)
    let system_schema = Arc::new(MemorySchemaProvider::new());
    let (_jobs_provider, schema_store, _schema_cache) =
        register_system_tables(&system_schema, backend.clone()).expect("register_system_tables failed");

    // Create a user table schema covering all types
    let ns = NamespaceId::from("ns_types");
    let tbl_name = TableName::from("dt_table");
    let table_id = TableId::new(ns.clone(), tbl_name.clone());

    let columns = vec![
        ColumnDefinition::primary_key("id", 1, KalamDataType::BigInt),
        ColumnDefinition::simple("b", 2, KalamDataType::Boolean),
        ColumnDefinition::simple("i", 3, KalamDataType::Int),
        ColumnDefinition::simple("bi", 4, KalamDataType::BigInt),
        ColumnDefinition::simple("d", 5, KalamDataType::Double),
        ColumnDefinition::simple("f", 6, KalamDataType::Float),
        ColumnDefinition::simple("s", 7, KalamDataType::Text),
        ColumnDefinition::simple("ts", 8, KalamDataType::Timestamp),
        ColumnDefinition::simple("date", 9, KalamDataType::Date),
        ColumnDefinition::simple("dt", 10, KalamDataType::DateTime), // DateTime with UTC timezone
        ColumnDefinition::simple("time", 11, KalamDataType::Time),
        ColumnDefinition::simple("json", 12, KalamDataType::Json),
        ColumnDefinition::simple("bytes", 13, KalamDataType::Bytes),
        ColumnDefinition::simple("emb", 14, KalamDataType::Embedding(4)),
        // P0 datatypes (UUID, DECIMAL, SMALLINT)
        ColumnDefinition::simple("uuid_val", 15, KalamDataType::Uuid),
        ColumnDefinition::simple("decimal_val", 16, KalamDataType::Decimal { precision: 10, scale: 2 }),
        ColumnDefinition::simple("smallint_val", 17, KalamDataType::SmallInt),
        // System columns typically exist; include them explicitly for the flush path
        ColumnDefinition::simple("_updated", 18, KalamDataType::Timestamp),
        ColumnDefinition::simple("_deleted", 19, KalamDataType::Boolean),
    ];

    let table_def = TableDefinition::new_with_defaults(
        ns.as_str(),
        tbl_name.as_str(),
        TableType::User,
        columns,
        Some("datatype preservation table".into()),
    )
    .expect("Failed to create table definition");

    // Store table definition (not strictly used by the job, but keeps catalog consistent)
    schema_store.put(&table_id, &table_def).expect("Failed to store table definition");

    // Arrow schema for the job
    let arrow_schema = table_def
        .to_arrow_schema()
        .expect("Failed to convert to Arrow schema");

    // Create user table store
    let store = Arc::new(new_user_table_store(backend.clone(), &ns, &tbl_name));

    // Insert deterministically-generated rows for user "userA"
    let user_id = "userA";
    let base_ts = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let base_date = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();

    let row_count = 50usize; // keep the test reasonably fast
    for i in 0..row_count {
        let id = i as i64;
        let b = i % 2 == 0;
        let i32v = i as i32 - 25;
        let i64v = (i as i64) * 1000 - 123;
        let d = (i as f64) / 3.0 + 0.125;
        let f = (i as f32) / 7.0 - 0.5;
        let s = format!("row-{}", i);
        let ts = (base_ts + Duration::seconds(i as i64)).timestamp_millis();
        let date_str = (base_date + Duration::days(i as i64)).format("%Y-%m-%d").to_string();
        let dt_str = (base_ts + Duration::minutes(i as i64)).to_rfc3339();
        let time_str = format!("12:34:{:02}.{:06}", (i % 60), (i * 137) % 1_000_000);
        let json_str = format!("{{\"k\":{}}}", i);
        let bytes: Vec<u8> = (0..8).map(|j| ((i + j) % 256) as u8).collect();
        let bytes_b64 = format!("base64:{}", general_purpose::STANDARD.encode(bytes.as_slice()));
        let emb = vec![i as f32, i as f32 + 0.5, -(i as f32), 1.0];
        
        // P0 datatype values
        // UUID: generate deterministic RFC 4122 formatted UUIDs
        let uuid_str = format!(
            "550e8400-e29b-41d4-a716-446655{:06x}",
            i % 0x1000000
        );
        // DECIMAL: generate monetary values like $1234.56 (precision=10, scale=2)
        let decimal_val = ((i as i64) * 100 + 1234_56) as f64 / 100.0;
        // SMALLINT: cover edge cases (-32768, 0, 32767)
        let smallint_val = if i == 0 {
            -32768i16
        } else if i == 1 {
            0i16
        } else if i == 2 {
            32767i16
        } else {
            ((i as i16) % 100) - 50
        };

        let fields = serde_json::json!({
            "id": id,
            "b": b,
            "i": i32v,
            "bi": i64v,
            "d": d,
            "f": f,
            "s": s,
            "ts": ts,
            "date": date_str,
            "dt": dt_str,
            "time": time_str,
            "json": json_str,
            "bytes": bytes_b64,
            "emb": emb,
            "uuid_val": uuid_str,
            "decimal_val": decimal_val,
            "smallint_val": smallint_val,
            "_updated": ts,
            "_deleted": false,
        });

        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), id.to_string());
        let row = UserTableRow {
            row_id: id.to_string(),
            user_id: user_id.to_string(),
            fields: serde_json::Value::Object(fields.as_object().cloned().unwrap()),
            _updated: base_ts.to_rfc3339(),
            _deleted: false,
        };
        store.put(&key, &row).expect("insert row failed");
    }

    // Define a storage output path template using ${user_id}
    let output_base = temp.path().join("parquet_out");
    let template = format!(
        "{}/{{}}/{{}}/${{user_id}}",
        output_base.to_string_lossy()
    );
    // Fill namespace/table placeholders (simple string replace)
    let template = template
        .replacen("{}", ns.as_str(), 1)
        .replacen("{}", tbl_name.as_str(), 1);

    // Execute flush job
    let job = UserTableFlushJob::new(
        store.clone(),
        ns.clone(),
        tbl_name.clone(),
        arrow_schema.clone(),
        template.clone(),
    );

    let result = job.execute_tracked().expect("flush job failed");
    assert_eq!(result.rows_flushed as usize, row_count, "unexpected rows_flushed");
    assert_eq!(result.parquet_files.len(), 1, "expected one parquet file (single user)");

    // Parquet file should exist
    let parquet_path = Path::new(&result.parquet_files[0]);
    assert!(parquet_path.exists(), "parquet file not found: {:?}", parquet_path);

    // Read back using Parquet reader and verify a few representative values across types
    {
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        let file = std::fs::File::open(parquet_path).expect("open parquet");
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).expect("parquet builder");
        let mut reader = builder.build().expect("parquet reader");

        let mut total_rows = 0usize;
        while let Some(batch) = reader.next() {
            let batch = batch.expect("read batch");
            total_rows += batch.num_rows();

            // Verify schema datatypes are correct (especially timezone for DateTime)
            let schema = batch.schema();
            let ts_field = schema.field_with_name("ts").unwrap();
            let dt_field = schema.field_with_name("dt").unwrap();
            let uuid_field = schema.field_with_name("uuid_val").unwrap();
            let decimal_field = schema.field_with_name("decimal_val").unwrap();
            let smallint_field = schema.field_with_name("smallint_val").unwrap();
            
            // Timestamp should have no timezone
            assert!(
                matches!(ts_field.data_type(), arrow::datatypes::DataType::Timestamp(arrow::datatypes::TimeUnit::Millisecond, None)),
                "Timestamp field should have no timezone, got {:?}", ts_field.data_type()
            );
            
            // DateTime should have UTC timezone
            assert!(
                matches!(dt_field.data_type(), arrow::datatypes::DataType::Timestamp(arrow::datatypes::TimeUnit::Millisecond, Some(tz)) if tz.as_ref() == "UTC"),
                "DateTime field should have UTC timezone, got {:?}", dt_field.data_type()
            );
            
            // UUID should be FixedSizeBinary(16)
            assert!(
                matches!(uuid_field.data_type(), arrow::datatypes::DataType::FixedSizeBinary(16)),
                "UUID field should be FixedSizeBinary(16), got {:?}", uuid_field.data_type()
            );
            
            // DECIMAL should be Decimal128(10, 2)
            assert!(
                matches!(decimal_field.data_type(), arrow::datatypes::DataType::Decimal128(10, 2)),
                "DECIMAL field should be Decimal128(10, 2), got {:?}", decimal_field.data_type()
            );
            
            // SMALLINT should be Int16
            assert!(
                matches!(smallint_field.data_type(), arrow::datatypes::DataType::Int16),
                "SMALLINT field should be Int16, got {:?}", smallint_field.data_type()
            );

            // Minimal field checks for a sample of rows in this batch
            let id_arr = batch
                .column(batch.schema().index_of("id").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::Int64Array>()
                .unwrap();
            let b_arr = batch
                .column(batch.schema().index_of("b").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::BooleanArray>()
                .unwrap();
            let i_arr = batch
                .column(batch.schema().index_of("i").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::Int32Array>()
                .unwrap();
            let d_arr = batch
                .column(batch.schema().index_of("d").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::Float64Array>()
                .unwrap();
            let f_arr = batch
                .column(batch.schema().index_of("f").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::Float32Array>()
                .unwrap();
            let s_arr = batch
                .column(batch.schema().index_of("s").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .unwrap();
            let ts_arr = batch
                .column(batch.schema().index_of("ts").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::TimestampMillisecondArray>()
                .unwrap();
            let date_arr = batch
                .column(batch.schema().index_of("date").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::Date32Array>()
                .unwrap();
            let dt_arr = batch
                .column(batch.schema().index_of("dt").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::TimestampMillisecondArray>()
                .unwrap();
            let time_arr = batch
                .column(batch.schema().index_of("time").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::Time64MicrosecondArray>()
                .unwrap();
            let bytes_arr = batch
                .column(batch.schema().index_of("bytes").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::BinaryArray>()
                .unwrap();
            let emb_arr = batch
                .column(batch.schema().index_of("emb").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::FixedSizeListArray>()
                .unwrap();
            let uuid_arr = batch
                .column(batch.schema().index_of("uuid_val").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::FixedSizeBinaryArray>()
                .unwrap();
            let decimal_arr = batch
                .column(batch.schema().index_of("decimal_val").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::Decimal128Array>()
                .unwrap();
            let smallint_arr = batch
                .column(batch.schema().index_of("smallint_val").unwrap())
                .as_any()
                .downcast_ref::<arrow::array::Int16Array>()
                .unwrap();

            for row in 0..batch.num_rows().min(5) { // sample subset per batch
                let id = id_arr.value(row);
                let i_usize = id as usize;

                // Recompute expected
                let exp_b = i_usize % 2 == 0;
                let exp_i = i_usize as i32 - 25;
                let exp_d = (i_usize as f64) / 3.0 + 0.125;
                let exp_f = (i_usize as f32) / 7.0 - 0.5;
                let exp_s = format!("row-{}", i_usize);
                let exp_ts = (base_ts + Duration::seconds(id as i64)).timestamp_millis();
                // date: compute days since epoch
                let exp_date_days = {
                    let date = base_date + Duration::days(id as i64);
                    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
                    (date - epoch).num_days() as i32
                };
                let exp_dt_ms = (base_ts + Duration::minutes(id as i64)).timestamp_millis();
                let exp_time_us = {
                    let secs = (i_usize % 60) as i64;
                    let micros = ((i_usize * 137) % 1_000_000) as i64;
                    (12 * 3600 + 34 * 60) as i64 * 1_000_000 + secs * 1_000_000 + micros
                };
                let exp_bytes: Vec<u8> = (0..8).map(|j| ((i_usize + j) % 256) as u8).collect();
                let exp_emb = [i_usize as f32, i_usize as f32 + 0.5, -(i_usize as f32), 1.0];
                
                // P0 datatype expected values
                let exp_uuid_bytes = {
                    let uuid_str = format!(
                        "550e8400-e29b-41d4-a716-446655{:06x}",
                        i_usize % 0x1000000
                    );
                    let clean = uuid_str.replace("-", "");
                    hex::decode(clean).expect("hex decode uuid")
                };
                let exp_decimal = ((i_usize as i64) * 100 + 1234_56) as i128; // stored as cents (scale=2)
                let exp_smallint = if i_usize == 0 {
                    -32768i16
                } else if i_usize == 1 {
                    0i16
                } else if i_usize == 2 {
                    32767i16
                } else {
                    ((i_usize as i16) % 100) - 50
                };

                assert_eq!(b_arr.value(row), exp_b, "bool mismatch for id={}", id);
                assert_eq!(i_arr.value(row), exp_i, "int mismatch for id={}", id);
                assert!((d_arr.value(row) - exp_d).abs() < 1e-9, "double mismatch for id={}", id);
                assert!((f_arr.value(row) - exp_f).abs() < 1e-6, "float mismatch for id={}", id);
                assert_eq!(s_arr.value(row), exp_s, "text mismatch for id={}", id);
                assert_eq!(ts_arr.value(row), exp_ts, "ts(ms) mismatch for id={}", id);
                assert_eq!(date_arr.value(row), exp_date_days, "date(days) mismatch for id={}", id);
                assert_eq!(dt_arr.value(row), exp_dt_ms, "datetime(ms) mismatch for id={}", id);
                assert_eq!(time_arr.value(row), exp_time_us, "time(us) mismatch for id={}", id);
                let got_bytes = bytes_arr.value(row);
                assert_eq!(got_bytes, exp_bytes.as_slice(), "bytes mismatch for id={}", id);
                
                // P0 datatype assertions
                let got_uuid = uuid_arr.value(row);
                assert_eq!(got_uuid, exp_uuid_bytes.as_slice(), "UUID mismatch for id={}", id);
                
                let got_decimal = decimal_arr.value(row);
                assert_eq!(got_decimal, exp_decimal, "DECIMAL mismatch for id={}", id);
                
                let got_smallint = smallint_arr.value(row);
                assert_eq!(got_smallint, exp_smallint, "SMALLINT mismatch for id={}", id);

                // Embedding: extract fixed-size list item values
                if emb_arr.is_valid(row) {
                    let values = emb_arr.values();
                    // Start offset = row * size
                    let size = emb_arr.value_length() as usize;
                    let start = row * size;
                    let vals = values
                        .as_any()
                        .downcast_ref::<arrow::array::Float32Array>()
                        .unwrap();
                    let got = [
                        vals.value(start),
                        vals.value(start + 1),
                        vals.value(start + 2),
                        vals.value(start + 3),
                    ];
                    for k in 0..4 {
                        assert!((got[k] - exp_emb[k]).abs() < 1e-6, "embedding[{}] mismatch for id={}", k, id);
                    }
                } else {
                    panic!("embedding null for id={}", id);
                }
            }
        }
        assert_eq!(total_rows, row_count, "row count mismatch");
    }

    // Also verify the file physically exists on disk under the computed directory
    let user_dir = output_base
        .join(ns.as_str())
        .join(tbl_name.as_str())
        .join(user_id);
    assert!(user_dir.exists(), "user dir missing: {:?}", user_dir);
    let files = fs::read_dir(&user_dir).unwrap();
    let parquet_files: Vec<_> = files
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|s| s == "parquet").unwrap_or(false))
        .collect();
    assert!(!parquet_files.is_empty(), "no parquet files found in {:?}", user_dir);
}
