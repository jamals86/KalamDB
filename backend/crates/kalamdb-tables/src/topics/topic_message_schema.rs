//! Topic message schema definition
//!
//! Provides cached Arrow schema for CONSUME query results using OnceLock singleton pattern.
//! This avoids reconstructing the schema on every CONSUME operation.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use std::sync::{Arc, OnceLock};

/// Topic message schema singleton
///
/// Returns cached Arrow schema for CONSUME FROM query results with 7 fields:
/// - topic: Utf8 (NOT NULL) - Topic name
/// - partition: Int32 (NOT NULL) - Partition ID
/// - offset: Int64 (NOT NULL) - Message offset
/// - key: Utf8 (NULLABLE) - Optional message key
/// - payload: Binary (NOT NULL) - Message payload bytes
/// - timestamp_ms: Int64 (NOT NULL) - Message timestamp in milliseconds
/// - op: Utf8 (NOT NULL) - Operation type (Insert, Update, Delete)
///
/// # Performance
/// Schema is constructed once and cached globally using `OnceLock`.
/// All subsequent calls return a cheap Arc clone (~8 bytes vs ~300 bytes for inline construction).
///
/// # Example
/// ```rust
/// use kalamdb_tables::topics::topic_message_schema;
///
/// let schema = topic_message_schema(); // First call: constructs schema
/// let schema2 = topic_message_schema(); // Subsequent calls: cheap Arc clone
/// ```
pub fn topic_message_schema() -> SchemaRef {
    static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
    SCHEMA
        .get_or_init(|| {
            Arc::new(Schema::new(vec![
                Field::new("topic", DataType::Utf8, false),
                Field::new("partition", DataType::Int32, false),
                Field::new("offset", DataType::Int64, false),
                Field::new("key", DataType::Utf8, true),
                Field::new("payload", DataType::Binary, false),
                Field::new("timestamp_ms", DataType::Int64, false),
                Field::new("op", DataType::Utf8, false),
            ]))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_message_schema() {
        let schema = topic_message_schema();
        assert_eq!(schema.fields().len(), 7);
        assert_eq!(schema.field(0).name(), "topic");
        assert_eq!(schema.field(0).is_nullable(), false);
        assert_eq!(schema.field(1).name(), "partition");
        assert_eq!(schema.field(2).name(), "offset");
        assert_eq!(schema.field(3).name(), "key");
        assert_eq!(schema.field(3).is_nullable(), true);
        assert_eq!(schema.field(4).name(), "payload");
        assert_eq!(schema.field(5).name(), "timestamp_ms");
        assert_eq!(schema.field(6).name(), "op");
        assert_eq!(schema.field(6).is_nullable(), false);
    }

    #[test]
    fn test_schema_singleton() {
        let schema1 = topic_message_schema();
        let schema2 = topic_message_schema();

        // Verify both calls return Arc pointers to same underlying schema
        assert!(Arc::ptr_eq(&schema1, &schema2));
    }
}
