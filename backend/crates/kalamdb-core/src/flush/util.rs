//! Flush utilities shared by user and shared table flushers.

use crate::error::KalamDbError;
use base64::{engine::general_purpose, Engine as _};
use chrono::Timelike;
use datafusion::arrow::array::{
    Array, ArrayData, ArrayRef, BinaryBuilder, BooleanBuilder, Date32Builder, Decimal128Builder,
    FixedSizeBinaryBuilder, FixedSizeListArray, FixedSizeListBuilder, Float32Builder,
    Float64Builder, Int16Builder, Int32Builder, Int64Builder, StringBuilder,
    Time64MicrosecondBuilder, TimestampMillisecondBuilder,
};
use datafusion::arrow::datatypes::{DataType, Field, SchemaRef, TimeUnit};
use datafusion::arrow::record_batch::RecordBatch;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Build an Arrow RecordBatch from JSON rows based on the provided schema.
/// TODO: Improve this to use streaming instead of loading all rows into memory
/// Supported types: Utf8, Int64, Float64, Boolean, Timestamp(Millisecond, None)
/// Streaming JSON batch builder that appends rows using Arrow builders,
/// avoiding an intermediate Vec of all rows.
pub struct JsonBatchBuilder {
    schema: SchemaRef,
    builders: Vec<(String, ColBuilder)>,
}

enum ColBuilder {
    Utf8(StringBuilder),
    I16(Int16Builder),
    I32(Int32Builder),
    Int64(Int64Builder),
    F32(Float32Builder),
    F64(Float64Builder),
    Bool(BooleanBuilder),
    TsMsNoTz(TimestampMillisecondBuilder),
    TsMsUtc(TimestampMillisecondBuilder),
    Date32(Date32Builder),
    Time64Us(Time64MicrosecondBuilder),
    Binary(BinaryBuilder),
    Uuid(FixedSizeBinaryBuilder),
    Decimal128 {
        builder: Decimal128Builder,
        precision: u8,
        scale: u8,
    },
    FixedSizeListF32 {
        builder: FixedSizeListBuilder<Float32Builder>,
        size: i32,
    },
}

impl JsonBatchBuilder {
    pub fn new(schema: SchemaRef) -> Result<Self, KalamDbError> {
        let mut builders = Vec::with_capacity(schema.fields().len());
        for f in schema.fields() {
            let name = f.name().clone();
            let b = match f.data_type() {
                DataType::Utf8 => ColBuilder::Utf8(StringBuilder::new()),
                DataType::Int16 => ColBuilder::I16(Int16Builder::new()),
                DataType::Int32 => ColBuilder::I32(Int32Builder::new()),
                DataType::Int64 => ColBuilder::Int64(Int64Builder::new()),
                DataType::Float32 => ColBuilder::F32(Float32Builder::new()),
                DataType::Float64 => ColBuilder::F64(Float64Builder::new()),
                DataType::Boolean => ColBuilder::Bool(BooleanBuilder::new()),
                DataType::Timestamp(TimeUnit::Millisecond, None) => {
                    ColBuilder::TsMsNoTz(TimestampMillisecondBuilder::new())
                }
                DataType::Timestamp(TimeUnit::Millisecond, Some(_)) => {
                    ColBuilder::TsMsUtc(TimestampMillisecondBuilder::new())
                }
                DataType::Date32 => ColBuilder::Date32(Date32Builder::new()),
                DataType::Time64(TimeUnit::Microsecond) => {
                    ColBuilder::Time64Us(Time64MicrosecondBuilder::new())
                }
                DataType::Binary => ColBuilder::Binary(BinaryBuilder::new()),
                DataType::FixedSizeBinary(16) => {
                    // UUID as FixedSizeBinary(16)
                    ColBuilder::Uuid(FixedSizeBinaryBuilder::new(16))
                }
                DataType::Decimal128(precision, scale) => {
                    // DECIMAL with precision and scale
                    ColBuilder::Decimal128 {
                        builder: Decimal128Builder::new()
                            .with_precision_and_scale(*precision, *scale)
                            .map_err(|e| {
                                KalamDbError::Other(format!("Invalid decimal params: {}", e))
                            })?,
                        precision: *precision,
                        scale: *scale as u8,
                    }
                }
                DataType::FixedSizeList(field, size) => {
                    // Support EMBEDDING as FixedSizeList<Float32>
                    if matches!(field.data_type(), DataType::Float32) {
                        let values = Float32Builder::new();
                        let inner_field = Field::new("item", DataType::Float32, false);
                        let list_builder =
                            FixedSizeListBuilder::new(values, *size).with_field(inner_field);
                        ColBuilder::FixedSizeListF32 {
                            builder: list_builder,
                            size: *size,
                        }
                    } else {
                        return Err(KalamDbError::Other(format!(
                            "Unsupported FixedSizeList type for flush: {:?}",
                            f.data_type()
                        )));
                    }
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
        let obj = row
            .as_object()
            .ok_or_else(|| KalamDbError::Other("Row is not a JSON object".to_string()))?;
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
                ColBuilder::I32(b) => {
                    if let Some(val) = v.and_then(|vv| vv.as_i64()) {
                        if val >= i32::MIN as i64 && val <= i32::MAX as i64 {
                            b.append_value(val as i32);
                        } else {
                            // Out of range -> null to avoid panic/corruption
                            b.append_null();
                        }
                    } else {
                        b.append_null();
                    }
                }
                ColBuilder::I16(b) => {
                    if let Some(val) = v.and_then(|vv| vv.as_i64()) {
                        if val >= i16::MIN as i64 && val <= i16::MAX as i64 {
                            b.append_value(val as i16);
                        } else {
                            // Out of range -> null to avoid panic/corruption
                            b.append_null();
                        }
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
                ColBuilder::F32(b) => {
                    if let Some(val) = v.and_then(|vv| vv.as_f64()) {
                        b.append_value(val as f32);
                    } else if let Some(val) = v.and_then(|vv| vv.as_i64()) {
                        b.append_value(val as f32);
                    } else {
                        b.append_null();
                    }
                }
                ColBuilder::F64(b) => {
                    if let Some(val) = v.and_then(|vv| vv.as_f64()) {
                        b.append_value(val);
                    } else if let Some(val) = v.and_then(|vv| vv.as_i64()) {
                        b.append_value(val as f64);
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
                ColBuilder::TsMsNoTz(b) | ColBuilder::TsMsUtc(b) => {
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
                ColBuilder::Date32(b) => {
                    if let Some(v) = v {
                        if let Some(days) = v.as_i64() {
                            if days >= i32::MIN as i64 && days <= i32::MAX as i64 {
                                b.append_value(days as i32);
                            } else {
                                b.append_null();
                            }
                        } else if let Some(s) = v.as_str() {
                            // Parse YYYY-MM-DD or RFC3339 and take date part
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                                let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
                                let days = (date - epoch).num_days() as i32;
                                b.append_value(days);
                            } else if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                                let date = dt.naive_utc().date();
                                let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
                                let days = (date - epoch).num_days() as i32;
                                b.append_value(days);
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
                ColBuilder::Time64Us(b) => {
                    if let Some(v) = v {
                        if let Some(us) = v.as_i64() {
                            b.append_value(us);
                        } else if let Some(s) = v.as_str() {
                            // Parse HH:MM:SS[.fffffffff]
                            if let Ok(t) = chrono::NaiveTime::parse_from_str(s, "%H:%M:%S%.f") {
                                let secs = t.num_seconds_from_midnight() as i64;
                                let nanos = t.nanosecond() as i64;
                                let micros = secs * 1_000_000 + nanos / 1_000;
                                b.append_value(micros);
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
                ColBuilder::Binary(b) => {
                    if let Some(v) = v {
                        if let Some(s) = v.as_str() {
                            // Accept "base64:..." or "hex:..." or attempt auto-detect base64; fallback to raw UTF-8 bytes
                            let bytes: Option<Vec<u8>> =
                                if let Some(rest) = s.strip_prefix("base64:") {
                                    general_purpose::STANDARD.decode(rest).ok()
                                } else if let Some(rest) = s.strip_prefix("hex:") {
                                    hex::decode(rest).ok()
                                } else {
                                    general_purpose::STANDARD.decode(s).ok().or_else(|| {
                                        // Fallback: raw bytes of the string
                                        Some(s.as_bytes().to_vec())
                                    })
                                };

                            if let Some(bytes) = bytes {
                                b.append_value(bytes.as_slice());
                            } else {
                                b.append_null();
                            }
                        } else if let Some(arr) = v.as_array() {
                            // Interpret as array of byte values
                            let mut ok = true;
                            let mut buf: Vec<u8> = Vec::with_capacity(arr.len());
                            for el in arr {
                                if let Some(n) = el.as_i64() {
                                    if (0..=255).contains(&n) {
                                        buf.push(n as u8);
                                    } else {
                                        ok = false;
                                        break;
                                    }
                                } else {
                                    ok = false;
                                    break;
                                }
                            }
                            if ok {
                                b.append_value(buf.as_slice());
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
                ColBuilder::Uuid(b) => {
                    if let Some(v) = v {
                        if let Some(s) = v.as_str() {
                            // Parse UUID from RFC 4122 format string (e.g., "550e8400-e29b-41d4-a716-446655440000")
                            // Remove hyphens and parse as hex
                            let cleaned = s.replace('-', "");
                            if cleaned.len() == 32 {
                                if let Ok(bytes) = hex::decode(&cleaned) {
                                    if bytes.len() == 16 {
                                        b.append_value(&bytes);
                                    } else {
                                        b.append_null();
                                    }
                                } else {
                                    b.append_null();
                                }
                            } else {
                                b.append_null();
                            }
                        } else if let Some(arr) = v.as_array() {
                            // Accept array of 16 bytes
                            if arr.len() == 16 {
                                let mut bytes = [0u8; 16];
                                let mut ok = true;
                                for (i, el) in arr.iter().enumerate() {
                                    if let Some(n) = el.as_i64() {
                                        if (0..=255).contains(&n) {
                                            bytes[i] = n as u8;
                                        } else {
                                            ok = false;
                                            break;
                                        }
                                    } else {
                                        ok = false;
                                        break;
                                    }
                                }
                                if ok {
                                    b.append_value(bytes);
                                } else {
                                    b.append_null();
                                }
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
                ColBuilder::Decimal128 {
                    builder,
                    precision,
                    scale,
                } => {
                    if let Some(v) = v {
                        // Parse decimal from number or string
                        let decimal_value: Option<i128> = if let Some(n) = v.as_f64() {
                            // Convert float to decimal by multiplying by 10^scale
                            let multiplier = 10_i128.pow(*scale as u32);
                            Some((n * multiplier as f64).round() as i128)
                        } else if let Some(n) = v.as_i64() {
                            // Integer value - multiply by 10^scale
                            let multiplier = 10_i128.pow(*scale as u32);
                            Some(n as i128 * multiplier)
                        } else if let Some(s) = v.as_str() {
                            // Parse string like "1234.56"
                            if let Ok(f) = s.parse::<f64>() {
                                let multiplier = 10_i128.pow(*scale as u32);
                                Some((f * multiplier as f64).round() as i128)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(val) = decimal_value {
                            // Validate precision (max digits)
                            let max_val = 10_i128.pow(*precision as u32) - 1;
                            if val.abs() <= max_val {
                                builder.append_value(val);
                            } else {
                                // Value exceeds precision - null
                                builder.append_null();
                            }
                        } else {
                            builder.append_null();
                        }
                    } else {
                        builder.append_null();
                    }
                }
                ColBuilder::FixedSizeListF32 { builder, size } => {
                    // Expect an array of float values with exact length = size
                    if let Some(v) = v {
                        if let Some(arr) = v.as_array() {
                            if arr.len() as i32 == *size {
                                let mut all_ok = true;
                                for el in arr {
                                    if let Some(fv) =
                                        el.as_f64().or_else(|| el.as_i64().map(|i| i as f64))
                                    {
                                        builder.values().append_value(fv as f32);
                                    } else {
                                        all_ok = false;
                                        break;
                                    }
                                }
                                if all_ok {
                                    builder.append(true);
                                } else {
                                    // Pad the remainder with zeros to satisfy fixed-size child length
                                    let already = arr
                                        .iter()
                                        .take_while(|el| {
                                            el.as_f64()
                                                .or_else(|| el.as_i64().map(|i| i as f64))
                                                .is_some()
                                        })
                                        .count();
                                    for _ in already..(*size as usize) {
                                        builder.values().append_value(0.0);
                                    }
                                    builder.append(false);
                                }
                            } else {
                                // Wrong length: append zeros and mark null
                                for _ in 0..(*size as usize) {
                                    builder.values().append_value(0.0);
                                }
                                builder.append(false);
                            }
                        } else {
                            // Not an array: append zeros and mark null
                            for _ in 0..(*size as usize) {
                                builder.values().append_value(0.0);
                            }
                            builder.append(false);
                        }
                    } else {
                        // Null: append zeros and mark null
                        for _ in 0..(*size as usize) {
                            builder.values().append_value(0.0);
                        }
                        builder.append(false);
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
                ColBuilder::I16(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::I32(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::Int64(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::F32(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::F64(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::Bool(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::TsMsNoTz(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::TsMsUtc(mut b) => Arc::new(b.finish().with_timezone("UTC")) as ArrayRef,
                ColBuilder::Date32(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::Time64Us(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::Binary(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::Uuid(mut b) => Arc::new(b.finish()) as ArrayRef,
                ColBuilder::Decimal128 { mut builder, .. } => {
                    Arc::new(builder.finish()) as ArrayRef
                }
                ColBuilder::FixedSizeListF32 { mut builder, size } => {
                    let arr = builder.finish();
                    // Rebuild ArrayData to enforce non-nullable inner field (Float32, nullable=false)
                    let data = arr.to_data();
                    let new_dt = DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float32, false)),
                        size,
                    );
                    let mut adb = ArrayData::builder(new_dt)
                        .len(data.len())
                        .offset(data.offset());
                    if let Some(nulls) = data.nulls() {
                        adb = adb.null_bit_buffer(Some(nulls.buffer().clone()));
                    }
                    for buf in data.buffers() {
                        adb = adb.add_buffer(buf.clone());
                    }
                    for child in data.child_data() {
                        adb = adb.add_child_data(child.clone());
                    }
                    let new_data = unsafe { adb.build_unchecked() };
                    Arc::new(FixedSizeListArray::from(new_data)) as ArrayRef
                }
            })
            .collect();
        RecordBatch::try_new(self.schema, arrays)
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }
}
