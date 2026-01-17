//! Arrow RecordBatch builder utilities for reducing boilerplate.
//!
//! This module provides convenient builders for creating Arrow RecordBatches
//! with common column types, reducing code duplication across system table
//! providers and other RecordBatch construction sites.

use arrow::array::{
    ArrayRef, BooleanArray, Float64Array, Int32Array, Int64Array, StringBuilder,
    TimestampMicrosecondArray, TimestampMillisecondArray, UInt64Array,
};
use arrow::datatypes::SchemaRef;
use arrow::error::ArrowError;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

/// Builder for constructing Arrow RecordBatches with type-safe column additions.
///
/// This builder simplifies the process of creating RecordBatches by providing
/// convenience methods for common column types used in KalamDB system tables.
///
/// # Example
///
/// ```rust,ignore
/// use kalamdb_commons::arrow_utils::RecordBatchBuilder;
///
/// let mut builder = RecordBatchBuilder::new(schema);
/// builder
///     .add_string_column(vec![Some("user1"), Some("user2"), None])
///     .add_int64_column(vec![Some(123), Some(456), Some(789)])
///     .add_timestamp_micros_column(vec![Some(1234567890), None, Some(9876543210)]);
///
/// let batch = builder.build()?;
/// ```
pub struct RecordBatchBuilder {
    schema: SchemaRef,
    columns: Vec<ArrayRef>,
}

impl RecordBatchBuilder {
    /// Create a new RecordBatchBuilder with the given schema.
    ///
    /// The schema defines the structure of the RecordBatch to be built.
    pub fn new(schema: SchemaRef) -> Self {
        let capacity = schema.fields().len();
        Self {
            schema,
            columns: Vec::with_capacity(capacity),
        }
    }

    fn push_array(&mut self, array: ArrayRef) -> &mut Self {
        self.columns.push(array);
        self
    }

    fn add_string_column_internal<I, S>(&mut self, data: I) -> &mut Self
    where
        I: IntoIterator<Item = Option<S>>,
        S: AsRef<str>,
    {
        let iter = data.into_iter();
        let (lower, upper) = iter.size_hint();
        let len = upper.unwrap_or(lower);
        let mut builder = StringBuilder::with_capacity(len, len * 32);
        for value in iter {
            match value {
                Some(s) => builder.append_value(s.as_ref()),
                None => builder.append_null(),
            }
        }
        self.push_array(Arc::new(builder.finish()))
    }

    /// Add a string column (UTF-8) to the batch.
    ///
    /// # Arguments
    /// * `data` - Vector of optional string values
    pub fn add_string_column(&mut self, data: Vec<Option<&str>>) -> &mut Self {
        self.add_string_column_internal(data)
    }

    /// Add a string column from owned Strings.
    ///
    /// # Arguments
    /// * `data` - Vector of optional String values
    pub fn add_string_column_owned(&mut self, data: Vec<Option<String>>) -> &mut Self {
        self.add_string_column_internal(data)
    }

    /// Add an Int64 column to the batch.
    ///
    /// # Arguments
    /// * `data` - Vector of optional i64 values
    pub fn add_int64_column(&mut self, data: Vec<Option<i64>>) -> &mut Self {
        self.push_array(Arc::new(Int64Array::from(data)))
    }

    /// Add a UInt64 column to the batch.
    ///
    /// # Arguments
    /// * `data` - Vector of optional u64 values
    pub fn add_uint64_column(&mut self, data: Vec<Option<u64>>) -> &mut Self {
        self.push_array(Arc::new(UInt64Array::from(data)))
    }

    /// Add an Int32 column to the batch.
    ///
    /// # Arguments
    /// * `data` - Vector of optional i32 values
    pub fn add_int32_column(&mut self, data: Vec<Option<i32>>) -> &mut Self {
        self.push_array(Arc::new(Int32Array::from(data)))
    }

    /// Add a Float64 column to the batch.
    ///
    /// # Arguments
    /// * `data` - Vector of optional f64 values
    pub fn add_float64_column(&mut self, data: Vec<Option<f64>>) -> &mut Self {
        self.push_array(Arc::new(Float64Array::from(data)))
    }

    /// Add a Boolean column to the batch.
    ///
    /// # Arguments
    /// * `data` - Vector of optional bool values
    pub fn add_boolean_column(&mut self, data: Vec<Option<bool>>) -> &mut Self {
        self.push_array(Arc::new(BooleanArray::from(data)))
    }

    /// Add a TimestampMicrosecond column to the batch.
    ///
    /// KalamDB stores timestamps as milliseconds but Arrow requires microseconds.
    /// This method automatically converts millisecond timestamps to microseconds.
    ///
    /// # Arguments
    /// * `data` - Vector of optional i64 values in **milliseconds**
    pub fn add_timestamp_micros_column(&mut self, data: Vec<Option<i64>>) -> &mut Self {
        let micros: Vec<Option<i64>> = data.into_iter().map(|ts| ts.map(|ms| ms * 1000)).collect();
        self.push_array(Arc::new(TimestampMicrosecondArray::from(micros)))
    }

    /// Add a TimestampMillisecond column to the batch.
    ///
    /// Use this for timestamps that are already in milliseconds and don't need conversion.
    ///
    /// # Arguments
    /// * `data` - Vector of optional i64 values in **milliseconds**
    pub fn add_timestamp_millis_column(&mut self, data: Vec<Option<i64>>) -> &mut Self {
        self.push_array(Arc::new(TimestampMillisecondArray::from(data)))
    }

    /// Add a pre-built array column directly.
    ///
    /// Use this for custom array types not covered by the convenience methods.
    ///
    /// # Arguments
    /// * `array` - Arc-wrapped array implementing the Array trait
    pub fn add_array_column(&mut self, array: ArrayRef) -> &mut Self {
        self.push_array(array)
    }

    /// Build the RecordBatch from accumulated columns.
    ///
    /// # Returns
    /// * `Ok(RecordBatch)` if the batch was built successfully
    /// * `Err(ArrowError)` if column count mismatch or other Arrow errors occur
    ///
    /// # Errors
    /// - Column count doesn't match schema field count
    /// - Column types don't match schema field types
    /// - Column lengths are inconsistent
    pub fn build(self) -> Result<RecordBatch, ArrowError> {
        RecordBatch::try_new(self.schema, self.columns)
    }

    /// Get the current number of columns added.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Get the expected number of columns from the schema.
    pub fn expected_column_count(&self) -> usize {
        self.schema.fields().len()
    }
}

/// Helper function to create an empty RecordBatch for a given schema.
///
/// Useful for queries that return no results but need a valid batch structure.
pub fn empty_batch(schema: SchemaRef) -> Result<RecordBatch, ArrowError> {
    let columns: Vec<ArrayRef> = schema
        .fields()
        .iter()
        .map(|field| arrow::array::new_empty_array(field.data_type()))
        .collect();
    RecordBatch::try_new(schema, columns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    fn test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("count", DataType::Int64, false),
            Field::new("created_at", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        ]))
    }

    #[test]
    fn test_basic_batch_builder() {
        let schema = test_schema();
        let mut builder = RecordBatchBuilder::new(schema);

        builder
            .add_string_column(vec![Some("id1"), Some("id2")])
            .add_int64_column(vec![Some(100), Some(200)])
            .add_timestamp_micros_column(vec![Some(1000), Some(2000)]);

        let batch = builder.build().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 3);
    }

    #[test]
    fn test_empty_batch() {
        let schema = test_schema();
        let batch = empty_batch(schema).unwrap();
        assert_eq!(batch.num_rows(), 0);
        assert_eq!(batch.num_columns(), 3);
    }

    #[test]
    fn test_column_count_mismatch() {
        let schema = test_schema();
        let mut builder = RecordBatchBuilder::new(schema);

        // Only add 2 columns when schema expects 3
        builder.add_string_column(vec![Some("id1")]).add_int64_column(vec![Some(100)]);

        assert!(builder.build().is_err());
    }

    #[test]
    fn test_timestamp_conversion() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )]));

        let mut builder = RecordBatchBuilder::new(schema);
        builder.add_timestamp_micros_column(vec![Some(1000)]); // 1000ms = 1000000μs

        let batch = builder.build().unwrap();
        let ts_array =
            batch.column(0).as_any().downcast_ref::<TimestampMicrosecondArray>().unwrap();

        assert_eq!(ts_array.value(0), 1000000); // Converted to microseconds
    }
}
