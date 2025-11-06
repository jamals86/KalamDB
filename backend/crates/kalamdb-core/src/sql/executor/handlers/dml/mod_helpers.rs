//! DML helpers (modular path)
use datafusion::scalar::ScalarValue;
use crate::error::KalamDbError;

pub fn validate_param_count(params:&[ScalarValue], expected:usize) -> Result<(),KalamDbError> {
    if params.len()!=expected { return Err(KalamDbError::InvalidOperation(format!("Parameter count mismatch: expected {} got {}", expected, params.len()))); }
    Ok(())
}

pub fn coerce_params(_params:&[ScalarValue]) -> Result<(),KalamDbError> { Ok(()) }
