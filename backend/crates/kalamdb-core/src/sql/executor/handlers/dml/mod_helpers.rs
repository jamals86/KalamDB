//! DML helpers (modular path)
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::providers::arrow_json_conversion::scalar_value_to_json;
use crate::sql::executor::default_evaluator::evaluate_default;
use crate::sql::executor::models::ScalarValue;
use kalamdb_commons::models::datatypes::KalamDataType;
use kalamdb_commons::models::UserId;
use kalamdb_commons::schemas::ColumnDefault;
use serde_json::Value as JsonValue;
use sqlparser::ast::{Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, Value};

pub fn validate_param_count(params: &[ScalarValue], expected: usize) -> Result<(), KalamDbError> {
    if params.len() != expected {
        return Err(KalamDbError::InvalidOperation(format!(
            "Parameter count mismatch: expected {} got {}",
            expected,
            params.len()
        )));
    }
    Ok(())
}

pub fn coerce_params(_params: &[ScalarValue]) -> Result<(), KalamDbError> {
    Ok(())
}

pub fn parse_placeholder_index(placeholder: &str) -> Result<usize, KalamDbError> {
    let stripped = placeholder.strip_prefix('$').ok_or_else(|| {
        KalamDbError::InvalidOperation(format!("Unsupported placeholder format: {}", placeholder))
    })?;

    let param_num: usize = stripped.parse().map_err(|_| {
        KalamDbError::InvalidOperation(format!("Invalid placeholder: {}", placeholder))
    })?;

    if param_num == 0 {
        return Err(KalamDbError::InvalidOperation(format!(
            "Invalid placeholder index: {}",
            placeholder
        )));
    }

    Ok(param_num)
}

pub fn scalar_from_placeholder(
    placeholder: &str,
    params: &[ScalarValue],
) -> Result<ScalarValue, KalamDbError> {
    let param_num = parse_placeholder_index(placeholder)?;
    if param_num > params.len() {
        return Err(KalamDbError::InvalidOperation(format!(
            "Parameter ${} out of range (have {} parameters)",
            param_num,
            params.len()
        )));
    }

    Ok(params[param_num - 1].clone())
}

pub fn scalar_from_sql_value(
    value: &Value,
    params: &[ScalarValue],
) -> Result<ScalarValue, KalamDbError> {
    match value {
        Value::Placeholder(ph) => scalar_from_placeholder(ph, params),
        Value::Number(n, _) => {
            if let Ok(i) = n.parse::<i64>() {
                Ok(ScalarValue::Int64(Some(i)))
            } else if let Ok(f) = n.parse::<f64>() {
                Ok(ScalarValue::Float64(Some(f)))
            } else {
                Err(KalamDbError::InvalidOperation(format!("Invalid number: {}", n)))
            }
        },
        Value::SingleQuotedString(s)
        | Value::DoubleQuotedString(s)
        | Value::EscapedStringLiteral(s)
        | Value::NationalStringLiteral(s) => Ok(ScalarValue::Utf8(Some(s.clone()))),
        Value::DollarQuotedString(s) => Ok(ScalarValue::Utf8(Some(s.value.clone()))),
        Value::HexStringLiteral(s) => Ok(ScalarValue::Utf8(Some(s.clone()))),
        Value::Boolean(b) => Ok(ScalarValue::Boolean(Some(*b))),
        Value::Null => Ok(ScalarValue::Null),
        _ => Ok(ScalarValue::Utf8(Some(value.to_string()))),
    }
}

pub fn coerce_scalar_to_type(
    value: ScalarValue,
    target: &KalamDataType,
) -> Result<ScalarValue, KalamDbError> {
    match (target, &value) {
        (KalamDataType::Uuid, ScalarValue::Utf8(Some(s))) => {
            let uuid = uuid::Uuid::parse_str(s).map_err(|e| {
                KalamDbError::InvalidOperation(format!("Invalid UUID string '{}': {}", s, e))
            })?;
            Ok(ScalarValue::FixedSizeBinary(16, Some(uuid.as_bytes().to_vec())))
        },
        (KalamDataType::Uuid, ScalarValue::Utf8(None)) => {
            Ok(ScalarValue::FixedSizeBinary(16, None))
        },
        _ => Ok(value),
    }
}

pub fn scalar_to_pk_string(value: &ScalarValue) -> Result<String, KalamDbError> {
    kalamdb_commons::scalar_to_pk_string(value)
        .map_err(|e| KalamDbError::InvalidOperation(format!("Primary key conversion error: {}", e)))
}

pub fn function_expr_to_scalar(
    func: &Function,
    params: &[ScalarValue],
    user_id: &UserId,
    app_context: &AppContext,
) -> Result<ScalarValue, KalamDbError> {
    let name = func.name.to_string();
    let args = function_args_to_json(func, params, user_id, app_context)?;
    let col_default = ColumnDefault::FunctionCall { name, args };
    let sys_cols = app_context.system_columns_service();
    evaluate_default(&col_default, user_id, Some(sys_cols))
}

pub fn function_args_to_json(
    func: &Function,
    params: &[ScalarValue],
    user_id: &UserId,
    app_context: &AppContext,
) -> Result<Vec<JsonValue>, KalamDbError> {
    let mut args = Vec::new();
    match &func.args {
        FunctionArguments::List(arg_list) => {
            for arg in &arg_list.args {
                match arg {
                    FunctionArg::Named { .. } => {
                        return Err(KalamDbError::InvalidOperation(
                            "Named function arguments not supported".into(),
                        ));
                    },
                    FunctionArg::ExprNamed { .. } => {
                        return Err(KalamDbError::InvalidOperation(
                            "Named function arguments not supported".into(),
                        ));
                    },
                    FunctionArg::Unnamed(arg_expr) => match arg_expr {
                        FunctionArgExpr::Expr(expr) => {
                            args.push(expr_to_json_arg(expr, params, user_id, app_context)?);
                        },
                        _ => {
                            return Err(KalamDbError::InvalidOperation(
                                "Unsupported function argument expression type".into(),
                            ));
                        },
                    },
                }
            }
        },
        FunctionArguments::None => {},
        _ => {
            return Err(KalamDbError::InvalidOperation(
                "Unsupported function argument format".into(),
            ));
        },
    }
    Ok(args)
}

pub fn expr_to_json_arg(
    expr: &Expr,
    params: &[ScalarValue],
    user_id: &UserId,
    app_context: &AppContext,
) -> Result<JsonValue, KalamDbError> {
    match expr {
        Expr::Value(val_with_span) => match &val_with_span.value {
            Value::Placeholder(ph) => {
                let value = scalar_from_placeholder(ph, params)?;
                scalar_value_to_json(&value).map_err(KalamDbError::from)
            },
            Value::Number(n, _) => {
                if let Ok(i) = n.parse::<i64>() {
                    Ok(JsonValue::Number(i.into()))
                } else if let Ok(f) = n.parse::<f64>() {
                    serde_json::Number::from_f64(f)
                        .map(JsonValue::Number)
                        .ok_or_else(|| KalamDbError::InvalidOperation("Invalid float value".into()))
                } else {
                    Err(KalamDbError::InvalidOperation(format!("Invalid number: {}", n)))
                }
            },
            Value::SingleQuotedString(s) | Value::DoubleQuotedString(s) => {
                Ok(JsonValue::String(s.clone()))
            },
            Value::Boolean(b) => Ok(JsonValue::Bool(*b)),
            Value::Null => Ok(JsonValue::Null),
            _ => Ok(JsonValue::String(val_with_span.to_string())),
        },
        Expr::Identifier(ident) if ident.value.starts_with('$') => {
            let value = scalar_from_placeholder(&ident.value, params)?;
            scalar_value_to_json(&value).map_err(KalamDbError::from)
        },
        Expr::Function(func) => {
            let scalar = function_expr_to_scalar(func, params, user_id, app_context)?;
            scalar_value_to_json(&scalar).map_err(KalamDbError::from)
        },
        _ => Ok(JsonValue::String(expr.to_string())),
    }
}

pub fn extract_pk_from_where_pair(
    where_pair: &Option<(String, String)>,
    pk_column: &str,
    params: &[ScalarValue],
) -> Result<Option<String>, KalamDbError> {
    if let Some((col, token)) = where_pair {
        if !col.eq_ignore_ascii_case(pk_column) {
            return Ok(None);
        }
        let t = token.trim().trim_end_matches(';').trim();

        if t.starts_with('$') {
            let scalar = scalar_from_placeholder(t, params)?;
            return Ok(Some(scalar_to_pk_string(&scalar)?));
        }

        let unquoted = t.trim_matches('"').trim_matches('\'');
        return Ok(Some(unquoted.to_string()));
    }
    Ok(None)
}
