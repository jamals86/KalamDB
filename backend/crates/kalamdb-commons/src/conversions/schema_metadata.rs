//! Arrow schema metadata helpers
//!
//! Centralized helpers for reading/writing Arrow Field metadata keys used by KalamDB.
//! This ensures consistent key names and serialization across the codebase.

use crate::models::datatypes::KalamDataType;
use arrow_schema::Field;

/// Metadata key for serialized KalamDataType.
pub const KALAM_DATA_TYPE_METADATA_KEY: &str = "kalam_data_type";
/// Metadata key for compact column definition flags (e.g. "pk,nonnull,unique").
pub const KALAM_COLUMN_DEF_METADATA_KEY: &str = "kalam_column_def";

/// Read `KalamDataType` from Arrow field metadata.
pub fn read_kalam_data_type_metadata(field: &Field) -> Option<KalamDataType> {
    field
        .metadata()
        .get(KALAM_DATA_TYPE_METADATA_KEY)
        .and_then(|s| serde_json::from_str::<KalamDataType>(s).ok())
}

/// Attach `KalamDataType` metadata to an Arrow field.
///
/// Preserves any existing metadata on the field.
pub fn with_kalam_data_type_metadata(mut field: Field, kalam_type: &KalamDataType) -> Field {
    let kalam_type_json =
        serde_json::to_string(kalam_type).unwrap_or_else(|_| "\"Text\"".to_string());
    let mut metadata = field.metadata().clone();
    metadata.insert(KALAM_DATA_TYPE_METADATA_KEY.to_string(), kalam_type_json);
    field = field.with_metadata(metadata);
    field
}

/// Read compact column definition flags from Arrow field metadata.
pub fn read_kalam_column_def_metadata(field: &Field) -> Option<String> {
    field
        .metadata()
        .get(KALAM_COLUMN_DEF_METADATA_KEY)
        .map(|s| s.to_string())
}

/// Attach compact column definition flags to an Arrow field.
///
/// Pass values like `"pk,nonnull,unique"` or `"nonnull"`.
pub fn with_kalam_column_def_metadata(mut field: Field, column_def: &str) -> Field {
    let mut metadata = field.metadata().clone();
    metadata.insert(
        KALAM_COLUMN_DEF_METADATA_KEY.to_string(),
        column_def.to_string(),
    );
    field = field.with_metadata(metadata);
    field
}
