//! SQL dialect compatibility helpers.
//!
//! This module provides utilities for mapping PostgreSQL/MySQL specific
//! data types into Arrow data types that KalamDB understands.  Centralising
//! these conversions keeps the CREATE TABLE parsers in sync across crates.

use anyhow::{anyhow, Result};
use arrow::datatypes::{DataType, IntervalUnit, TimeUnit};
use sqlparser::ast::{DataType as SQLDataType, ObjectName};

/// Map a parsed `sqlparser` data type into an Arrow data type while accounting
/// for PostgreSQL/MySQL aliases (e.g. `SERIAL`, `INT4`, `AUTO_INCREMENT`).
pub fn map_sql_type_to_arrow(sql_type: &SQLDataType) -> Result<DataType> {
    use SQLDataType::*;

    let dtype = match sql_type {
        // Signed integers ----------------------------------------------------
        SmallInt(_) | Int2(_) => DataType::Int16,
        Int(_) | Integer(_) | Int4(_) => DataType::Int32,
        MediumInt(_) => DataType::Int32,
        BigInt(_) | Int8(_) | Int64 => DataType::Int64,
        TinyInt(_) => DataType::Int8,

        // Unsigned integers --------------------------------------------------
        UnsignedTinyInt(_) => DataType::UInt8,
        UnsignedSmallInt(_) | UnsignedInt2(_) => DataType::UInt16,
        UnsignedMediumInt(_) => DataType::UInt32,
        UnsignedInt(_) | UnsignedInteger(_) | UnsignedInt4(_) => DataType::UInt32,
        UnsignedBigInt(_) | UnsignedInt8(_) => DataType::UInt64,

        // Floating point -----------------------------------------------------
        Float(_) | Real | Float4 => DataType::Float32,
        Double | DoublePrecision | Float8 | Float64 => DataType::Float64,

        // Boolean ------------------------------------------------------------
        Boolean | Bool => DataType::Boolean,

        // Character / string -------------------------------------------------
        Character(_)
        | Char(_)
        | CharacterVarying(_)
        | CharVarying(_)
        | Varchar(_)
        | Nvarchar(_)
        | CharacterLargeObject(_)
        | CharLargeObject(_)
        | Clob(_)
        | Text
        | String(_)
        | JSON
        | JSONB => DataType::Utf8,

        // Binary -------------------------------------------------------------
        Binary(_) | Varbinary(_) | Blob(_) | Bytes(_) | Bytea => DataType::Binary,

        // Temporal -----------------------------------------------------------
        Date => DataType::Date32,
        Timestamp(_, _) | Datetime(_) => DataType::Timestamp(TimeUnit::Millisecond, None),
        Time(_, _) => DataType::Time64(TimeUnit::Nanosecond),
        Interval => DataType::Interval(IntervalUnit::MonthDayNano),

        // Custom or dialect specific identifiers ----------------------------
        Custom(name, _) => map_custom_type(name)?,

        // Struct / collection types -----------------------------------------
        Array(_) | Enum(_) | Set(_) | Struct(_) => DataType::Utf8,

        // Otherwise, leave unsupported so callers can surface a friendly error
        Numeric(_) | Decimal(_) | Dec(_) | BigNumeric(_) | BigDecimal(_) | Uuid | Regclass
        | Unspecified => {
            return Err(anyhow!("Unsupported data type: {:?}", sql_type));
        }
    };

    Ok(dtype)
}

fn map_custom_type(name: &ObjectName) -> Result<DataType> {
    let ident = name
        .0
        .iter()
        .map(|id| id.value.to_lowercase())
        .collect::<Vec<_>>()
        .join(".");

    let dtype = match ident.as_str() {
        // PostgreSQL serial aliases
        "serial" | "serial4" => DataType::Int32,
        "bigserial" | "serial8" => DataType::Int64,
        "smallserial" | "serial2" => DataType::Int16,
        // Postgres integer aliases
        "int1" => DataType::Int8,
        "int2" => DataType::Int16,
        "int4" => DataType::Int32,
        "int8" => DataType::Int64,
        // MySQL aliases
        "signed" => DataType::Int32,
        "unsigned" => DataType::UInt32,
        // Fallback to treating unknown custom types as UTF8 strings
        other if other.ends_with("text") || other.ends_with("string") => DataType::Utf8,
        other => {
            return Err(anyhow!("Unsupported custom data type '{}'", other));
        }
    };

    Ok(dtype)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::Ident;

    fn custom(name: &str) -> SQLDataType {
        SQLDataType::Custom(ObjectName(vec![Ident::new(name)]), vec![])
    }

    #[test]
    fn maps_postgres_serial_types() {
        assert_eq!(
            map_sql_type_to_arrow(&custom("serial")).unwrap(),
            DataType::Int32
        );
        assert_eq!(
            map_sql_type_to_arrow(&custom("serial8")).unwrap(),
            DataType::Int64
        );
        assert_eq!(
            map_sql_type_to_arrow(&custom("smallserial")).unwrap(),
            DataType::Int16
        );
    }

    #[test]
    fn maps_unsigned_variants() {
        assert_eq!(
            map_sql_type_to_arrow(&SQLDataType::UnsignedInt(None)).unwrap(),
            DataType::UInt32
        );
        assert_eq!(
            map_sql_type_to_arrow(&SQLDataType::UnsignedBigInt(None)).unwrap(),
            DataType::UInt64
        );
    }

    #[test]
    fn rejects_unknown_custom_types() {
        let err = map_sql_type_to_arrow(&custom("geography")).unwrap_err();
        assert!(err.to_string().contains("Unsupported custom data type"));
    }
}
