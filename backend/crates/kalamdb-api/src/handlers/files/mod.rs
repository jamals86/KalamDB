//! File download handlers for FILE datatype
//!
//! ## Endpoints
//! - GET /v1/files/{namespace}/{table_name}/{subfolder}/{file_id} - Download a file
//!
//! ## Download Flow
//! 1. Parse path parameters (namespace, table_name, subfolder, file_id)
//! 2. Check user permissions for the table
//! 3. Build FileRef path from parameters
//! 4. Stream file from storage with proper Content-Type

pub mod models;

mod download;

pub use download::download_file;
