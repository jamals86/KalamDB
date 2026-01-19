use crate::error::KalamDbError;
use crate::sql::executor::models::SessionUserContext;
use datafusion::catalog::Session;
use datafusion::logical_expr::{Expr, Operator};
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::{ReadContext, Role, UserId};
use once_cell::sync::Lazy;

static SYSTEM_USER_ID: Lazy<UserId> = Lazy::new(|| UserId::from("_system"));

fn is_seq_column(expr: &Expr) -> bool {
    matches!(expr, Expr::Column(col) if col.name == SystemColumnNames::SEQ)
}

fn extract_i64_literal(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Literal(ScalarValue::Int64(Some(val)), _) => Some(*val),
        _ => None,
    }
}

fn invert_comparison_operator(op: Operator) -> Option<Operator> {
    match op {
        Operator::Eq => Some(Operator::Eq),
        Operator::Gt => Some(Operator::Lt),
        Operator::GtEq => Some(Operator::LtEq),
        Operator::Lt => Some(Operator::Gt),
        Operator::LtEq => Some(Operator::GtEq),
        _ => None,
    }
}

fn normalize_seq_comparison(
    binary: &datafusion::logical_expr::BinaryExpr,
) -> Option<(Operator, i64)> {
    if is_seq_column(&binary.left) {
        let value = extract_i64_literal(&binary.right)?;
        return Some((binary.op, value));
    }

    if is_seq_column(&binary.right) {
        let value = extract_i64_literal(&binary.left)?;
        let op = invert_comparison_operator(binary.op)?;
        return Some((op, value));
    }

    None
}

/// Shared core state for all table providers
/// Returns (since_seq, until_seq)
/// since_seq is exclusive (>), until_seq is inclusive (<=)
/// For equality (_seq = X), returns (X-1, X) to match exactly that value
pub fn extract_seq_bounds_from_filter(expr: &Expr) -> (Option<SeqId>, Option<SeqId>) {
    match expr {
        Expr::BinaryExpr(binary) => match binary.op {
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
            },
            Operator::Eq | Operator::Gt | Operator::GtEq | Operator::Lt | Operator::LtEq => {
                let (op, val) = match normalize_seq_comparison(binary) {
                    Some(result) => result,
                    None => return (None, None),
                };

                match op {
                    Operator::Eq => {
                        let since = val.saturating_sub(1);
                        (Some(SeqId::from(since)), Some(SeqId::from(val)))
                    },
                    Operator::Gt | Operator::GtEq => {
                        let since = if op == Operator::Gt {
                            val
                        } else {
                            val.saturating_sub(1)
                        };
                        (Some(SeqId::from(since)), None)
                    },
                    Operator::Lt | Operator::LtEq => {
                        let until = if op == Operator::Lt {
                            val.saturating_sub(1)
                        } else {
                            val
                        };
                        (None, Some(SeqId::from(until)))
                    },
                    _ => (None, None),
                }
            },
            _ => (None, None),
        },
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
pub fn extract_user_context(state: &dyn Session) -> Result<(&UserId, Role), KalamDbError> {
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

    Ok((&user_ctx.user_id, user_ctx.role))
}

/// Extract full session context (user_id, role, read_context) from DataFusion SessionState extensions.
///
/// Use this when you need to check read routing (leader-only reads in Raft cluster mode).
pub fn extract_full_user_context(
    state: &dyn Session,
) -> Result<(&UserId, Role, ReadContext), KalamDbError> {
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

    Ok((&user_ctx.user_id, user_ctx.role, user_ctx.read_context))
}
