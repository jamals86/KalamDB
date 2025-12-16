use crate::error::KalamDbError;
use datafusion::catalog::Session;
use datafusion::logical_expr::{Expr, Operator};
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::{Role, UserId};
use once_cell::sync::Lazy;

static SYSTEM_USER_ID: Lazy<UserId> = Lazy::new(|| UserId::from("_system"));

/// Shared core state for all table providers
/// Returns (since_seq, until_seq)
/// since_seq is exclusive (>), until_seq is inclusive (<=)
/// For equality (_seq = X), returns (X-1, X) to match exactly that value
pub fn extract_seq_bounds_from_filter(expr: &Expr) -> (Option<SeqId>, Option<SeqId>) {
    match expr {
        Expr::BinaryExpr(binary) => {
            match binary.op {
                Operator::Eq => {
                    // Handle _seq = X (equality)
                    // Convert to range: since_seq = X-1 (exclusive), until_seq = X (inclusive)
                    // This matches only rows where _seq == X
                    if let Expr::Column(col) = &*binary.left {
                        if col.name == SystemColumnNames::SEQ {
                            if let Expr::Literal(ScalarValue::Int64(Some(val)), _) = &*binary.right
                            {
                                // since_seq is exclusive (>), so X-1 means only X and above
                                // until_seq is inclusive (<=), so X means only X and below
                                return (Some(SeqId::from(*val - 1)), Some(SeqId::from(*val)));
                            }
                        }
                    }
                    (None, None)
                }
                Operator::Gt | Operator::GtEq => {
                    // Handle _seq > X or _seq >= X
                    if let Expr::Column(col) = &*binary.left {
                        if col.name == SystemColumnNames::SEQ {
                            if let Expr::Literal(ScalarValue::Int64(Some(val)), _) = &*binary.right
                            {
                                let seq_val = if binary.op == Operator::Gt {
                                    *val
                                } else {
                                    *val - 1
                                };
                                return (Some(SeqId::from(seq_val)), None);
                            }
                        }
                    }
                    (None, None)
                }
                Operator::Lt | Operator::LtEq => {
                    // Handle _seq < X or _seq <= X
                    if let Expr::Column(col) = &*binary.left {
                        if col.name == SystemColumnNames::SEQ {
                            if let Expr::Literal(ScalarValue::Int64(Some(val)), _) = &*binary.right
                            {
                                let seq_val = if binary.op == Operator::Lt {
                                    *val - 1
                                } else {
                                    *val
                                };
                                return (None, Some(SeqId::from(seq_val)));
                            }
                        }
                    }
                    (None, None)
                }
                Operator::And => {
                    // Combine bounds from AND expressions
                    let (min_l, max_l) = extract_seq_bounds_from_filter(&binary.left);
                    let (min_r, max_r) = extract_seq_bounds_from_filter(&binary.right);

                    let min = match (min_l, min_r) {
                        (Some(a), Some(b)) => Some(if a > b { a } else { b }), // Max of mins
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        (None, None) => None,
                    };

                    let max = match (max_l, max_r) {
                        (Some(a), Some(b)) => Some(if a < b { a } else { b }), // Min of maxes
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        (None, None) => None,
                    };
                    (min, max)
                }
                _ => (None, None),
            }
        }
        _ => (None, None),
    }
}

/// Return a shared `_system` user identifier for scope-agnostic operations
pub fn system_user_id() -> &'static UserId {
    &SYSTEM_USER_ID
}

/// Resolve user scope, defaulting to the shared system identifier for scope-less tables
pub fn resolve_user_scope(scope: Option<&UserId>) -> &UserId {
    scope.unwrap_or_else(|| system_user_id())
}

/// Extract (user_id, role) from DataFusion SessionState extensions.
pub fn extract_user_context(state: &dyn Session) -> Result<(UserId, Role), KalamDbError> {
    use crate::sql::executor::models::SessionUserContext;

    let session_state = state
        .as_any()
        .downcast_ref::<datafusion::execution::context::SessionState>()
        .ok_or_else(|| KalamDbError::InvalidOperation("Expected SessionState".to_string()))?;

    let user_ctx = session_state
        .config()
        .options()
        .extensions
        .get::<SessionUserContext>()
        .ok_or_else(|| {
            KalamDbError::InvalidOperation("SessionUserContext not found in extensions".to_string())
        })?;

    Ok((user_ctx.user_id.clone(), user_ctx.role))
}
