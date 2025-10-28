//! Flush utilities shared by user and shared table flushers.

use crate::error::KalamDbError;
use datafusion::arrow::array::{
    ArrayRef, BooleanBuilder, Int64Builder, StringBuilder, TimestampMillisecondBuilder,
};
use datafusion::arrow::datatypes::{DataType, SchemaRef, TimeUnit};
use datafusion::arrow::record_batch::RecordBatch;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Build an Arrow RecordBatch from JSON rows based on the provided schema.
/// TODO: Improve this to use streaming instead of loading all rows into memory
/// Supported types: Utf8, Int64, Boolean, Timestamp(Millisecond, None)
/// Streaming JSON batch builder that appends rows using Arrow builders,
/// avoiding an intermediate Vec of all rows.
pub struct JsonBatchBuilder {
    schema: SchemaRef,
    builders: Vec<(String, ColBuilder)>,
}

enum ColBuilder {
    Utf8(StringBuilder),
    Int64(Int64Builder),
    Bool(BooleanBuilder),
    TsMs(TimestampMillisecondBuilder),
}

impl JsonBatchBuilder {
    pub fn new(schema: SchemaRef) -> Result<Self, KalamDbError> {
        let mut builders = Vec::with_capacity(schema.fields().len());
        for f in schema.fields() {
            let name = f.name().clone();
            let b = match f.data_type() {
                DataType::Utf8 => ColBuilder::Utf8(StringBuilder::new()),
                DataType::Int64 => ColBuilder::Int64(Int64Builder::new()),
                DataType::Boolean => ColBuilder::Bool(BooleanBuilder::new()),
                DataType::Timestamp(TimeUnit::Millisecond, None) => {
                    ColBuilder::TsMs(TimestampMillisecondBuilder::new())
                }
                other => {
                    return Err(KalamDbError::Other(format!(
                        "Unsupported data type for flush: {:?}",
                        other
                    )))
                }
            };
            builders.push((name, b));
        }
        Ok(Self { schema, builders })
    }

    pub fn push_object_row(&mut self, row: &JsonValue) -> Result<(), KalamDbError> {
        let obj = row.as_object().ok_or_else(|| {
            KalamDbError::Other("Row is not a JSON object".to_string())
        })?;
        for (name, builder) in &mut self.builders {
            let v = obj.get(name);
            match builder {
                ColBuilder::Utf8(b) => {
                    if let Some(JsonValue::String(s)) = v {
                        b.append_value(s);
                    } else if let Some(val) = v.and_then(|vv| vv.as_str()) {
                        b.append_value(val);
                    } else {
                        b.append_null();
                    }
                }
                ColBuilder::Int64(b) => {
                    if let Some(val) = v.and_then(|vv| vv.as_i64()) {
                        b.append_value(val);
                    } else {
                        b.append_null();
                    }
                }
                ColBuilder::Bool(b) => {
                    if let Some(val) = v.and_then(|vv| vv.as_bool()) {
                        b.append_value(val);
                    } else {
                        b.append_null();
                    }
                }
                ColBuilder::TsMs(b) => {
                    if let Some(vvv) = v {
                        if let Some(ms) = vvv.as_i64() {
                            b.append_value(ms);
                        } else if let Some(s) = vvv.as_str() {
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                                b.append_value(dt.timestamp_millis());
                            } else {
                                b.append_null();
                            }
                        } else {
                            b.append_null();
                        }
                    } else {
                        b.append_null();
                    }
                }
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Result<RecordBatch, KalamDbError> {
        let arrays: Vec<ArrayRef> = self
            .builders
            .into_iter()
            .map(|(_, b)| match b {
                ColBuilder::Utf8(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::Int64(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::Bool(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::TsMs(mut b) => Arc::new(b.finish()) as ArrayRef,
            })
            .collect();
        RecordBatch::try_new(self.schema, arrays)
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }
}

