use datafusion::logical_expr::Expr;
use kalamdb_commons::models::UserId;
use kalamdb_commons::{TableId, TableType};

/// Typed FDW scan input before it is converted into a backend request.
#[derive(Debug, Clone)]
pub struct ScanInput {
    pub table_id: TableId,
    pub table_type: TableType,
    pub projected_columns: Vec<String>,
    pub filters: Vec<Expr>,
    pub limit: Option<usize>,
    pub session_user_id: Option<UserId>,
}
