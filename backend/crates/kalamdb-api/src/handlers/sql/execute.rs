//! SQL execution handler for the `/v1/api/sql` REST API endpoint

use actix_multipart::Multipart;
use actix_web::{post, web, Either, FromRequest, HttpRequest, HttpResponse, Responder};
use kalamdb_auth::AuthSessionExtractor;
use kalamdb_commons::models::{NamespaceId, UserId, Username};
use kalamdb_commons::schemas::TableType;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::context::ExecutionContext;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::sql::SqlImpersonationService;
use kalamdb_raft::GroupId;
use kalamdb_session::AuthSession;
use kalamdb_system::FileSubfolderState;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use super::file_utils::{
    extract_file_placeholders, extract_table_from_sql, parse_sql_payload, stage_and_finalize_files,
    substitute_file_placeholders,
};
use super::forward::{forward_sql_if_follower, handle_not_leader_error};
use super::helpers::{cleanup_files, execute_single_statement, parse_scalar_params};
use super::models::{ErrorCode, QueryRequest, QueryResult, SqlResponse};
use crate::limiter::RateLimiter;

const EXECUTE_AS_PREFIX: &str = "EXECUTE AS USER";

#[derive(Debug)]
struct ParsedExecutionStatement {
    sql: String,
    execute_as_username: Option<Username>,
}

fn parse_execute_statement(statement: &str) -> Result<ParsedExecutionStatement, String> {
    let trimmed = statement.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return Err("Empty SQL statement".to_string());
    }

    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with(EXECUTE_AS_PREFIX) {
        return Ok(ParsedExecutionStatement {
            sql: trimmed.to_string(),
            execute_as_username: None,
        });
    }

    let after_prefix = trimmed[EXECUTE_AS_PREFIX.len()..].trim_start();
    let after_quote = after_prefix
        .strip_prefix('\'')
        .ok_or_else(|| "EXECUTE AS USER requires a single-quoted username".to_string())?;
    let end_quote = after_quote
        .find('\'')
        .ok_or_else(|| "EXECUTE AS USER username quote was not closed".to_string())?;
    let username = after_quote[..end_quote].trim();
    if username.is_empty() {
        return Err("EXECUTE AS USER username cannot be empty".to_string());
    }

    let after_username = after_quote[end_quote + 1..].trim_start();
    if !after_username.starts_with('(') {
        return Err("EXECUTE AS USER must wrap SQL in parentheses".to_string());
    }

    let mut depth = 0usize;
    let mut close_idx = None;
    for (idx, ch) in after_username.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Err("EXECUTE AS USER contains unbalanced parentheses".to_string());
                }
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(idx);
                    break;
                }
            },
            _ => {},
        }
    }

    let close_idx = close_idx.ok_or_else(|| "EXECUTE AS USER missing closing ')'".to_string())?;
    let inner_sql = after_username[1..close_idx].trim();
    if inner_sql.is_empty() {
        return Err("EXECUTE AS USER requires a non-empty inner SQL statement".to_string());
    }

    let trailing = after_username[close_idx + 1..].trim();
    if !trailing.is_empty() {
        return Err("EXECUTE AS USER must contain exactly one wrapped SQL statement".to_string());
    }

    let inner_statements = kalamdb_sql::split_statements(inner_sql)
        .map_err(|e| format!("Failed to parse inner SQL for EXECUTE AS USER: {}", e))?;
    if inner_statements.len() != 1 {
        return Err("EXECUTE AS USER can only wrap a single SQL statement".to_string());
    }

    Ok(ParsedExecutionStatement {
        sql: inner_statements[0].trim().to_string(),
        execute_as_username: Some(
            Username::try_new(username)
                .map_err(|e| format!("Invalid execute-as username: {}", e))?,
        ),
    })
}

fn authorized_username(exec_ctx: &ExecutionContext) -> Username {
    if let Some(username) = &exec_ctx.user_context().username {
        return username.clone();
    }

    Username::from(exec_ctx.user_id().as_str())
}

fn resolve_result_username(
    authorized_username: &Username,
    execute_as_username: Option<&Username>,
) -> Username {
    execute_as_username.cloned().unwrap_or_else(|| authorized_username.clone())
}

/// POST /v1/api/sql - Execute SQL statement(s)
///
/// Accepts either JSON or multipart/form-data payloads.
///
/// - JSON: `sql` plus optional `params` and `namespace_id`.
/// - Multipart: `sql`, optional `params` (JSON array), optional `namespace_id`,
///   and file parts named `file:<placeholder>` for FILE("name") placeholders.
///
/// Multiple statements can be separated by semicolons and will be executed sequentially.
/// File uploads require a single SQL statement.
///
/// # Authentication
/// Requires authentication via Authorization header with Bearer token.
/// Basic auth is not supported for this endpoint - use tokens only.
#[post("/sql")]
pub async fn execute_sql_v1(
    extractor: AuthSessionExtractor,
    http_req: HttpRequest,
    payload: web::Payload,
    app_context: web::Data<Arc<AppContext>>,
    sql_executor: web::Data<Arc<SqlExecutor>>,
    rate_limiter: Option<web::Data<Arc<RateLimiter>>>,
) -> impl Responder {
    let start_time = Instant::now();

    // Convert extractor to AuthSession
    let session: AuthSession = extractor.into();

    // Rate limiting: Check if user can execute query
    if let Some(ref limiter) = rate_limiter {
        if !limiter.check_query_rate(session.user_id()) {
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            log::warn!(
                "Rate limit exceeded for user: {} (queries per second)",
                session.user_id().as_str()
            );
            return HttpResponse::TooManyRequests().json(SqlResponse::error(
                ErrorCode::RateLimitExceeded,
                "Too many queries per second. Please slow down.",
                took,
            ));
        }
    }

    let content_type = http_req
        .headers()
        .get(actix_web::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let is_multipart = content_type.to_ascii_lowercase().starts_with("multipart/form-data");

    let mut payload = payload.into_inner();

    let parsed_payload = if is_multipart {
        let multipart = Multipart::new(http_req.headers(), payload);
        match parse_sql_payload(Either::Right(multipart), &app_context.config().files).await {
            Ok(p) => p,
            Err(e) => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest()
                    .json(SqlResponse::error(e.code, &e.message, took));
            },
        }
    } else {
        let json = match web::Json::<QueryRequest>::from_request(&http_req, &mut payload).await {
            Ok(j) => j,
            Err(e) => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error(
                    ErrorCode::InvalidInput,
                    &format!("Invalid JSON payload: {}", e),
                    took,
                ));
            },
        };
        match parse_sql_payload(Either::Left(json), &app_context.config().files).await {
            Ok(p) => p,
            Err(e) => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest()
                    .json(SqlResponse::error(e.code, &e.message, took));
            },
        }
    };

    let sql = parsed_payload.sql;
    let params_json = parsed_payload.params;
    let namespace_id = parsed_payload.namespace_id;
    let mut files = parsed_payload.files;
    let is_multipart = parsed_payload.is_multipart;

    let default_namespace = namespace_id.clone().unwrap_or_else(|| NamespaceId::new("default"));
    let base_session = app_context.base_session_context();

    // Create ExecutionContext from session and set namespace
    let exec_ctx = ExecutionContext::from_session(session, Arc::clone(&base_session))
        .with_namespace_id(default_namespace.clone());
    let authorized_username = authorized_username(&exec_ctx);
    let impersonation_service = SqlImpersonationService::new(Arc::clone(app_context.get_ref()));

    let files_present = files.as_ref().map(|f| !f.is_empty()).unwrap_or(false);
    if files_present {
        let executor = app_context.executor();
        if !executor.is_leader(GroupId::Meta).await {
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            return HttpResponse::ServiceUnavailable().json(SqlResponse::error(
                ErrorCode::NotLeader,
                "File uploads must be sent to the current leader",
                took,
            ));
        }
    }

    let req_for_forward = QueryRequest {
        sql: sql.clone(),
        params: params_json.clone(),
        namespace_id: namespace_id.clone(),
    };

    if !files_present {
        if let Some(response) = forward_sql_if_follower(
            &http_req,
            &req_for_forward,
            app_context.get_ref(),
            &default_namespace,
        )
        .await
        {
            return response;
        }
    }

    // Parse parameters if provided
    let params = match parse_scalar_params(&params_json) {
        Ok(p) => p,
        Err(err) => {
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            return HttpResponse::BadRequest().json(SqlResponse::error(
                ErrorCode::InvalidParameter,
                &err,
                took,
            ));
        },
    };

    // Handle FILE uploads (multipart only)
    let required_files = extract_file_placeholders(&sql);
    if !required_files.is_empty() || files_present {
        if !is_multipart {
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            return HttpResponse::BadRequest().json(SqlResponse::error(
                ErrorCode::InvalidInput,
                "FILE placeholders require multipart/form-data",
                took,
            ));
        }

        let statements = match kalamdb_sql::split_statements(&sql) {
            Ok(stmts) => stmts,
            Err(err) => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error(
                    ErrorCode::BatchParseError,
                    &format!("Failed to parse SQL batch: {}", err),
                    took,
                ));
            },
        };

        if statements.len() != 1 {
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            return HttpResponse::BadRequest().json(SqlResponse::error(
                ErrorCode::InvalidInput,
                "File uploads require a single SQL statement",
                took,
            ));
        }

        let parsed_statement = match parse_execute_statement(&statements[0]) {
            Ok(parsed) => parsed,
            Err(err) => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error(
                    ErrorCode::InvalidInput,
                    &err,
                    took,
                ));
            },
        };

        let execute_as_user = match parsed_statement.execute_as_username.as_ref() {
            Some(target_username) => {
                match impersonation_service.resolve_execute_as_user(
                    exec_ctx.user_id(),
                    exec_ctx.user_role(),
                    target_username.as_str(),
                ) {
                    Ok(user_id) => Some(user_id),
                    Err(err) => {
                        let took = start_time.elapsed().as_secs_f64() * 1000.0;
                        return HttpResponse::BadRequest().json(SqlResponse::error(
                            ErrorCode::SqlExecutionError,
                            &err.to_string(),
                            took,
                        ));
                    },
                }
            },
            None => None,
        };

        let mut files_map = files.take().unwrap_or_default();
        if !required_files.is_empty() {
            files_map =
                files_map.into_iter().filter(|(key, _)| required_files.contains(key)).collect();
        }

        let table_id = match extract_table_from_sql(
            &parsed_statement.sql,
            default_namespace.as_str(),
        ) {
            Some(tid) => tid,
            None => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error(
                    ErrorCode::InvalidInput,
                    "Could not determine target table from SQL. Use fully qualified table name (namespace.table).",
                    took,
                ));
            },
        };

        let schema_registry = app_context.schema_registry();
        let table_entry = match schema_registry.get_table_entry(&table_id) {
            Some(entry) => entry,
            None => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error(
                    ErrorCode::TableNotFound,
                    &format!("Table '{}' not found", table_id),
                    took,
                ));
            },
        };

        let storage_id = table_entry.storage_id.clone();
        let table_type = table_entry.table_type;

        let user_id = match table_type {
            TableType::User => execute_as_user.clone().or_else(|| Some(exec_ctx.user_id().clone())),
            TableType::Shared => None,
            TableType::Stream | TableType::System => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error(
                    ErrorCode::InvalidInput,
                    "File uploads are not supported for stream or system tables",
                    took,
                ));
            },
        };

        let manifest_service = app_context.manifest_service();
        let mut subfolder_state = match manifest_service.get_file_subfolder_state(&table_id) {
            Ok(Some(state)) => state,
            Ok(None) => FileSubfolderState::new(),
            Err(e) => {
                log::warn!("Failed to get subfolder state for {}: {}", table_id, e);
                FileSubfolderState::new()
            },
        };

        let file_service = app_context.file_storage_service();
        let file_refs = if files_map.is_empty() {
            HashMap::new()
        } else {
            match stage_and_finalize_files(
                file_service.as_ref(),
                &files_map,
                &storage_id,
                table_type,
                &table_id,
                user_id.as_ref(),
                &mut subfolder_state,
                None,
            )
            .await
            {
                Ok(refs) => refs,
                Err(e) => {
                    let took = start_time.elapsed().as_secs_f64() * 1000.0;
                    return HttpResponse::InternalServerError()
                        .json(SqlResponse::error(e.code, &e.message, took));
                },
            }
        };

        let modified_sql = substitute_file_placeholders(&parsed_statement.sql, &file_refs);

        let effective_username = parsed_statement.execute_as_username.as_ref();
        let effective_username = resolve_result_username(&authorized_username, effective_username);

        return match execute_single_statement(
            &modified_sql,
            app_context.get_ref(),
            sql_executor.get_ref(),
            &exec_ctx,
            execute_as_user,
            None,
            params,
        )
        .await
        {
            Ok(result) => {
                let result = result.with_as_user(effective_username);
                if let Err(e) =
                    manifest_service.update_file_subfolder_state(&table_id, subfolder_state)
                {
                    log::warn!("Failed to update subfolder state for {}: {}", table_id, e);
                }

                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                HttpResponse::Ok().json(SqlResponse::success(vec![result], took))
            },
            Err(err) => {
                cleanup_files(
                    &file_refs,
                    &storage_id,
                    table_type,
                    &table_id,
                    user_id.as_ref(),
                    app_context.get_ref(),
                )
                .await;
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                HttpResponse::BadRequest().json(SqlResponse::error_with_details(
                    ErrorCode::SqlExecutionError,
                    &format!("Statement 1 failed: {}", err),
                    &modified_sql,
                    took,
                ))
            },
        };
    }

    // Parse SQL statements
    let statements = match kalamdb_sql::split_statements(&sql) {
        Ok(stmts) => stmts,
        Err(err) => {
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            return HttpResponse::BadRequest().json(SqlResponse::error(
                ErrorCode::BatchParseError,
                &format!("Failed to parse SQL batch: {}", err),
                took,
            ));
        },
    };

    if statements.is_empty() {
        let took = start_time.elapsed().as_secs_f64() * 1000.0;
        return HttpResponse::BadRequest().json(SqlResponse::error(
            ErrorCode::EmptySql,
            "No SQL statements provided",
            took,
        ));
    }

    // Reject multi-statement batches with parameters
    if !params.is_empty() && statements.len() > 1 {
        let took = start_time.elapsed().as_secs_f64() * 1000.0;
        return HttpResponse::BadRequest().json(SqlResponse::error(
            ErrorCode::ParamsWithBatch,
            "Parameters not supported with multi-statement batches",
            took,
        ));
    }

    // Execute statements
    let mut results = Vec::new();
    let mut total_inserted = 0usize;
    let mut total_updated = 0usize;
    let mut total_deleted = 0usize;

    for (idx, sql) in statements.iter().enumerate() {
        let parsed_statement = match parse_execute_statement(sql) {
            Ok(parsed) => parsed,
            Err(err) => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error(
                    ErrorCode::InvalidInput,
                    &err,
                    took,
                ));
            },
        };

        let execute_as_user: Option<UserId> = match parsed_statement.execute_as_username.as_ref() {
            Some(target_username) => {
                match impersonation_service.resolve_execute_as_user(
                    exec_ctx.user_id(),
                    exec_ctx.user_role(),
                    target_username.as_str(),
                ) {
                    Ok(user_id) => Some(user_id),
                    Err(err) => {
                        let took = start_time.elapsed().as_secs_f64() * 1000.0;
                        return HttpResponse::BadRequest().json(SqlResponse::error(
                            ErrorCode::SqlExecutionError,
                            &err.to_string(),
                            took,
                        ));
                    },
                }
            },
            None => None,
        };

        let stmt_start = Instant::now();
        let effective_username = parsed_statement.execute_as_username.as_ref();
        let effective_username = resolve_result_username(&authorized_username, effective_username);
        match execute_single_statement(
            &parsed_statement.sql,
            app_context.get_ref(),
            sql_executor.get_ref(),
            &exec_ctx,
            execute_as_user,
            None,
            params.clone(),
        )
        .await
        {
            Ok(result) => {
                let result = result.with_as_user(effective_username);
                // Calculate timing and row count
                let stmt_duration_secs = stmt_start.elapsed().as_secs_f64();
                let stmt_duration_ms = stmt_duration_secs * 1000.0;
                let row_count = result.rows.as_ref().map(|r| r.len()).unwrap_or(0);

                // SECURITY: Redact sensitive data before logging
                let safe_sql =
                    kalamdb_commons::helpers::security::redact_sensitive_sql(&parsed_statement.sql);
                log::debug!(
                    target: "sql::exec",
                    "✅ SQL executed | sql='{}' | user='{}' | role='{:?}' | rows={} | took={:.3}ms",
                    safe_sql,
                    exec_ctx.user_id().as_str(),
                    exec_ctx.user_role(),
                    row_count,
                    stmt_duration_ms
                );

                // Log slow query if threshold exceeded
                app_context.slow_query_logger().log_if_slow(
                    safe_sql,
                    stmt_duration_secs,
                    row_count,
                    exec_ctx.user_id().clone(),
                    kalamdb_core::schema_registry::TableType::User,
                    None,
                );

                // Accumulate DML row counts for multi-statement batches
                if statements.len() > 1 {
                    if let Some(ref msg) = result.message {
                        if msg.contains("Inserted") {
                            total_inserted += result.row_count;
                            continue;
                        } else if msg.contains("Updated") {
                            total_updated += result.row_count;
                            continue;
                        } else if msg.contains("Deleted") {
                            total_deleted += result.row_count;
                            continue;
                        }
                    }
                }

                results.push(result);
            },
            Err(err) => {
                // Check if NOT_LEADER error and auto-forward to leader
                if let Some(kalamdb_err) = err.downcast_ref::<kalamdb_core::error::KalamDbError>() {
                    if let Some(response) = handle_not_leader_error(
                        kalamdb_err,
                        &http_req,
                        &req_for_forward,
                        app_context.get_ref(),
                        start_time,
                    )
                    .await
                    {
                        return response;
                    }
                }

                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error_with_details(
                    ErrorCode::SqlExecutionError,
                    &format!("Statement {} failed: {}", idx + 1, err),
                    &parsed_statement.sql,
                    took,
                ));
            },
        }
    }

    // Add accumulated DML results for multi-statement batches
    if statements.len() > 1 {
        if total_inserted > 0 {
            results.push(
                QueryResult::with_affected_rows(
                    total_inserted,
                    Some(format!("Inserted {} row(s)", total_inserted)),
                )
                .with_as_user(authorized_username.clone()),
            );
        }
        if total_updated > 0 {
            results.push(
                QueryResult::with_affected_rows(
                    total_updated,
                    Some(format!("Updated {} row(s)", total_updated)),
                )
                .with_as_user(authorized_username.clone()),
            );
        }
        if total_deleted > 0 {
            results.push(
                QueryResult::with_affected_rows(
                    total_deleted,
                    Some(format!("Deleted {} row(s)", total_deleted)),
                )
                .with_as_user(authorized_username.clone()),
            );
        }
    }

    let took = start_time.elapsed().as_secs_f64() * 1000.0;
    HttpResponse::Ok().json(SqlResponse::success(results, took))
}

#[cfg(test)]
mod tests {
    use super::{parse_execute_statement, resolve_result_username};
    use kalamdb_commons::models::Username;

    #[test]
    fn parse_execute_as_user_wrapper() {
        let parsed = parse_execute_statement(
            "EXECUTE AS USER 'alice' (SELECT * FROM default.todos WHERE id = 1);",
        )
        .expect("wrapper should parse");

        assert_eq!(parsed.execute_as_username, Some(Username::from("alice")));
        assert_eq!(parsed.sql, "SELECT * FROM default.todos WHERE id = 1");
    }

    #[test]
    fn reject_multi_statement_inside_wrapper() {
        let err = parse_execute_statement("EXECUTE AS USER 'alice' (SELECT 1; SELECT 2)")
            .expect_err("multiple statements should be rejected");
        assert!(err.contains("single SQL statement"));
    }

    #[test]
    fn passthrough_non_wrapper_statement() {
        let parsed = parse_execute_statement("SELECT * FROM default.todos WHERE id = 10")
            .expect("statement should pass through");
        assert!(parsed.execute_as_username.is_none());
        assert_eq!(parsed.sql, "SELECT * FROM default.todos WHERE id = 10");
    }

    #[test]
    fn resolve_result_username_uses_authorized_when_no_execute_as() {
        let authorized = Username::from("admin_user");
        let actual = resolve_result_username(&authorized, None);
        assert_eq!(actual, authorized);
    }

    #[test]
    fn resolve_result_username_uses_execute_as_when_present() {
        let authorized = Username::from("admin_user");
        let execute_as = Username::from("alice");
        let actual = resolve_result_username(&authorized, Some(&execute_as));
        assert_eq!(actual, execute_as);
    }
}
