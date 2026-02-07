pub mod reader;
pub mod writer;

pub use reader::{
    parse_parquet_from_bytes, parse_parquet_schema_from_bytes, read_parquet_batches,
    read_parquet_batches_sync, read_parquet_schema, read_parquet_schema_sync,
};
pub use writer::ParquetWriteResult;
