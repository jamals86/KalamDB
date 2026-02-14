use std::collections::BTreeMap;

use datafusion::scalar::ScalarValue;

use crate::ids::SeqId;
use crate::models::rows::{Row, StoredScalarValue, UserTableRow};
use crate::models::UserId;
use crate::serialization::generated::row_models_generated::kalamdb::serialization::row as fb_row;
use crate::serialization::schema::{
    ROW_SCHEMA_ID, ROW_SCHEMA_VERSION, SHARED_TABLE_ROW_SCHEMA_ID, USER_TABLE_ROW_SCHEMA_ID,
};
use crate::serialization::{decode_enveloped, encode_enveloped, CodecKind};
use crate::storage::StorageError;

type Result<T> = std::result::Result<T, StorageError>;

pub fn encode_row(row: &Row) -> Result<Vec<u8>> {
    let mut builder = flatbuffers::FlatBufferBuilder::new();

    let mut column_offsets = Vec::with_capacity(row.values.len());
    for (name, value) in row.values.iter() {
        let stored = StoredScalarValue::from(value);
        let payload_bytes = flexbuffers::to_vec(stored.clone()).map_err(|e| {
            StorageError::SerializationError(format!("row scalar encode failed: {e}"))
        })?;
        let payload_vec = builder.create_vector(&payload_bytes);

        let scalar = fb_row::ScalarValuePayload::create(
            &mut builder,
            &fb_row::ScalarValuePayloadArgs {
                tag: to_scalar_tag(&stored),
                payload: Some(payload_vec),
            },
        );
        let name_offset = builder.create_string(name);
        let col = fb_row::ColumnValue::create(
            &mut builder,
            &fb_row::ColumnValueArgs {
                name: Some(name_offset),
                value: Some(scalar),
            },
        );
        column_offsets.push(col);
    }

    let columns = builder.create_vector(&column_offsets);
    let row_payload = fb_row::RowPayload::create(
        &mut builder,
        &fb_row::RowPayloadArgs {
            columns: Some(columns),
        },
    );

    fb_row::finish_row_payload_buffer(&mut builder, row_payload);
    encode_enveloped(
        CodecKind::FlatBuffers,
        ROW_SCHEMA_ID,
        ROW_SCHEMA_VERSION,
        builder.finished_data().to_vec(),
    )
}

pub fn decode_row(bytes: &[u8]) -> Result<Row> {
    let envelope = decode_enveloped(bytes, ROW_SCHEMA_ID, ROW_SCHEMA_VERSION)?;
    let payload = envelope.payload;

    if !fb_row::row_payload_buffer_has_identifier(&payload) {
        return Err(StorageError::SerializationError(
            "row decode failed: invalid file identifier (expected KROW)".to_string(),
        ));
    }

    let row = fb_row::root_as_row_payload(&payload)
        .map_err(|e| StorageError::SerializationError(format!("row decode failed: {e}")))?;

    decode_row_payload(row)
}

pub fn encode_user_table_row(row: &UserTableRow) -> Result<Vec<u8>> {
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let user_id = builder.create_string(row.user_id.as_str());
    let fields = encode_row_payload_table(&mut builder, &row.fields)?;

    let payload = fb_row::UserTableRowPayload::create(
        &mut builder,
        &fb_row::UserTableRowPayloadArgs {
            user_id: Some(user_id),
            seq: row._seq.as_i64(),
            deleted: row._deleted,
            fields: Some(fields),
        },
    );
    builder.finish(payload, None);
    encode_enveloped(
        CodecKind::FlatBuffers,
        USER_TABLE_ROW_SCHEMA_ID,
        ROW_SCHEMA_VERSION,
        builder.finished_data().to_vec(),
    )
}

pub fn decode_user_table_row(bytes: &[u8]) -> Result<UserTableRow> {
    let envelope = decode_enveloped(bytes, USER_TABLE_ROW_SCHEMA_ID, ROW_SCHEMA_VERSION)?;
    let payload =
        flatbuffers::root::<fb_row::UserTableRowPayload>(&envelope.payload).map_err(|e| {
            StorageError::SerializationError(format!("user table row decode failed: {e}"))
        })?;

    let user_id = payload.user_id().ok_or_else(|| {
        StorageError::SerializationError(
            "user table row decode failed: missing user_id".to_string(),
        )
    })?;
    let fields = payload.fields().ok_or_else(|| {
        StorageError::SerializationError("user table row decode failed: missing fields".to_string())
    })?;

    Ok(UserTableRow {
        user_id: UserId::from(user_id),
        _seq: SeqId::new(payload.seq()),
        _deleted: payload.deleted(),
        fields: decode_row_payload(fields)?,
    })
}

pub fn encode_shared_table_row(seq: SeqId, deleted: bool, fields: &Row) -> Result<Vec<u8>> {
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let fields = encode_row_payload_table(&mut builder, fields)?;

    let payload = fb_row::SharedTableRowPayload::create(
        &mut builder,
        &fb_row::SharedTableRowPayloadArgs {
            seq: seq.as_i64(),
            deleted,
            fields: Some(fields),
        },
    );
    builder.finish(payload, None);
    encode_enveloped(
        CodecKind::FlatBuffers,
        SHARED_TABLE_ROW_SCHEMA_ID,
        ROW_SCHEMA_VERSION,
        builder.finished_data().to_vec(),
    )
}

pub fn decode_shared_table_row(bytes: &[u8]) -> Result<(SeqId, bool, Row)> {
    let envelope = decode_enveloped(bytes, SHARED_TABLE_ROW_SCHEMA_ID, ROW_SCHEMA_VERSION)?;
    let payload =
        flatbuffers::root::<fb_row::SharedTableRowPayload>(&envelope.payload).map_err(|e| {
            StorageError::SerializationError(format!("shared table row decode failed: {e}"))
        })?;
    let fields = payload.fields().ok_or_else(|| {
        StorageError::SerializationError(
            "shared table row decode failed: missing fields".to_string(),
        )
    })?;

    Ok((SeqId::new(payload.seq()), payload.deleted(), decode_row_payload(fields)?))
}

fn encode_row_payload_table<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    row: &Row,
) -> Result<flatbuffers::WIPOffset<fb_row::RowPayload<'a>>> {
    let mut column_offsets = Vec::with_capacity(row.values.len());
    for (name, value) in row.values.iter() {
        let stored = StoredScalarValue::from(value);
        let payload_bytes = flexbuffers::to_vec(stored.clone()).map_err(|e| {
            StorageError::SerializationError(format!("row scalar encode failed: {e}"))
        })?;
        let payload_vec = builder.create_vector(&payload_bytes);
        let scalar = fb_row::ScalarValuePayload::create(
            builder,
            &fb_row::ScalarValuePayloadArgs {
                tag: to_scalar_tag(&stored),
                payload: Some(payload_vec),
            },
        );
        let name_offset = builder.create_string(name);
        let col = fb_row::ColumnValue::create(
            builder,
            &fb_row::ColumnValueArgs {
                name: Some(name_offset),
                value: Some(scalar),
            },
        );
        column_offsets.push(col);
    }

    let columns = builder.create_vector(&column_offsets);
    Ok(fb_row::RowPayload::create(
        builder,
        &fb_row::RowPayloadArgs {
            columns: Some(columns),
        },
    ))
}

fn decode_row_payload(payload: fb_row::RowPayload<'_>) -> Result<Row> {
    let mut values = BTreeMap::<String, ScalarValue>::new();
    if let Some(columns) = payload.columns() {
        for col in columns.iter() {
            let name = col.name().ok_or_else(|| {
                StorageError::SerializationError(
                    "row decode failed: column missing name".to_string(),
                )
            })?;
            let scalar = col.value().ok_or_else(|| {
                StorageError::SerializationError(
                    "row decode failed: column missing scalar value".to_string(),
                )
            })?;
            let payload_bytes = scalar.payload().ok_or_else(|| {
                StorageError::SerializationError(
                    "row decode failed: scalar payload missing".to_string(),
                )
            })?;
            let stored: StoredScalarValue = flexbuffers::from_slice(payload_bytes.bytes())
                .map_err(|e| {
                    StorageError::SerializationError(format!("row scalar decode failed: {e}"))
                })?;

            if to_scalar_tag(&stored) != scalar.tag() {
                return Err(StorageError::SerializationError(format!(
                    "row decode failed: scalar tag mismatch for column '{name}'"
                )));
            }

            values.insert(name.to_string(), ScalarValue::from(stored));
        }
    }
    Ok(Row { values })
}

fn to_scalar_tag(value: &StoredScalarValue) -> fb_row::ScalarTag {
    match value {
        StoredScalarValue::Null => fb_row::ScalarTag::Null,
        StoredScalarValue::Boolean(_) => fb_row::ScalarTag::Boolean,
        StoredScalarValue::Float32(_) => fb_row::ScalarTag::Float32,
        StoredScalarValue::Float64(_) => fb_row::ScalarTag::Float64,
        StoredScalarValue::Int8(_) => fb_row::ScalarTag::Int8,
        StoredScalarValue::Int16(_) => fb_row::ScalarTag::Int16,
        StoredScalarValue::Int32(_) => fb_row::ScalarTag::Int32,
        StoredScalarValue::Int64(_) => fb_row::ScalarTag::Int64,
        StoredScalarValue::UInt8(_) => fb_row::ScalarTag::UInt8,
        StoredScalarValue::UInt16(_) => fb_row::ScalarTag::UInt16,
        StoredScalarValue::UInt32(_) => fb_row::ScalarTag::UInt32,
        StoredScalarValue::UInt64(_) => fb_row::ScalarTag::UInt64,
        StoredScalarValue::Utf8(_) => fb_row::ScalarTag::Utf8,
        StoredScalarValue::LargeUtf8(_) => fb_row::ScalarTag::LargeUtf8,
        StoredScalarValue::Binary(_) => fb_row::ScalarTag::Binary,
        StoredScalarValue::LargeBinary(_) => fb_row::ScalarTag::LargeBinary,
        StoredScalarValue::FixedSizeBinary { .. } => fb_row::ScalarTag::FixedSizeBinary,
        StoredScalarValue::Date32(_) => fb_row::ScalarTag::Date32,
        StoredScalarValue::Time64Microsecond(_) => fb_row::ScalarTag::Time64Microsecond,
        StoredScalarValue::TimestampMillisecond { .. } => fb_row::ScalarTag::TimestampMillisecond,
        StoredScalarValue::TimestampMicrosecond { .. } => fb_row::ScalarTag::TimestampMicrosecond,
        StoredScalarValue::TimestampNanosecond { .. } => fb_row::ScalarTag::TimestampNanosecond,
        StoredScalarValue::Decimal128 { .. } => fb_row::ScalarTag::Decimal128,
        StoredScalarValue::Embedding { .. } => fb_row::ScalarTag::Embedding,
        StoredScalarValue::Fallback(_) => fb_row::ScalarTag::Fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::rows::UserTableRow;
    use std::sync::Arc;

    #[test]
    fn row_roundtrip_mixed_types() {
        let mut values = BTreeMap::new();
        values.insert("id".to_string(), ScalarValue::Int64(Some(42)));
        values.insert("name".to_string(), ScalarValue::Utf8(Some("alice".to_string())));
        values.insert("active".to_string(), ScalarValue::Boolean(Some(true)));
        values.insert("notes".to_string(), ScalarValue::Utf8(None));
        values.insert(
            "ts".to_string(),
            ScalarValue::TimestampMillisecond(Some(1000), Some(Arc::<str>::from("UTC"))),
        );

        let row = Row { values };
        let encoded = encode_row(&row).expect("encode row");
        let decoded = decode_row(&encoded).expect("decode row");
        assert_eq!(decoded, row);
    }

    #[test]
    fn user_table_row_roundtrip() {
        let mut values = BTreeMap::new();
        values.insert("pk".to_string(), ScalarValue::Int64(Some(7)));
        let fields = Row { values };

        let row = UserTableRow {
            user_id: UserId::from("user1"),
            _seq: SeqId::new(123),
            _deleted: false,
            fields,
        };

        let encoded = encode_user_table_row(&row).expect("encode user row");
        let decoded = decode_user_table_row(&encoded).expect("decode user row");
        assert_eq!(decoded, row);
    }

    #[test]
    fn shared_table_row_roundtrip() {
        let mut values = BTreeMap::new();
        values.insert("pk".to_string(), ScalarValue::Int64(Some(11)));
        values.insert("name".to_string(), ScalarValue::Utf8(Some("shared".to_string())));
        let fields = Row { values };

        let encoded =
            encode_shared_table_row(SeqId::new(456), true, &fields).expect("encode shared row");
        let (seq, deleted, decoded_fields) =
            decode_shared_table_row(&encoded).expect("decode shared row");
        assert_eq!(seq, SeqId::new(456));
        assert!(deleted);
        assert_eq!(decoded_fields, fields);
    }
}
