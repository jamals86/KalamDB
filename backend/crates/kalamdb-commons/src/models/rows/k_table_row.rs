use crate::ids::SeqId;
use crate::models::UserId;
use super::row::Row;
use serde::{Deserialize, Serialize};

/// Unified table row model for User and Stream tables
///
/// **Phase 13: Provider Consolidation**
/// - Unifies UserTableRow and StreamTableRow
/// - Used by BaseTableProvider::snapshot_rows
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KTableRow {
    pub user_id: UserId,
    pub _seq: SeqId,
    /// Soft delete flag (always false for stream tables)
    pub _deleted: bool,
    /// Row data (JSON)
    pub fields: Row,
}

impl KTableRow {
    pub fn new(user_id: UserId, _seq: SeqId, fields: Row, _deleted: bool) -> Self {
        Self {
            user_id,
            _seq,
            _deleted,
            fields,
        }
    }
}
