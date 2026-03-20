use datafusion::physical_plan::collect;
use kalam_pg_api::{
    DeleteRequest, InsertRequest, KalamBackendExecutor, MutationResponse, ScanRequest,
    ScanResponse, TenantContext, UpdateRequest,
};
use kalam_pg_common::KalamPgError;
use kalamdb_commons::models::UserId;
use kalamdb_commons::Role;
use kalamdb_commons::TableType;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::ExecutionContext;
use kalamdb_tables::utils::row_utils::system_user_id;
use std::sync::Arc;

/// Embedded runtime wrapper around the existing shared [`AppContext`].
pub struct EmbeddedKalamRuntime {
    app_context: Arc<AppContext>,
}

impl EmbeddedKalamRuntime {
    /// Wrap an already-initialized [`AppContext`].
    pub fn from_app_context(app_context: Arc<AppContext>) -> Result<Self, KalamPgError> {
        Ok(Self { app_context })
    }

    /// Access the shared application context.
    pub fn app_context(&self) -> &Arc<AppContext> {
        &self.app_context
    }

    fn execution_context(&self, tenant_context: &TenantContext) -> ExecutionContext {
        match tenant_context.effective_user_id() {
            Some(user_id) => ExecutionContext::new(
                user_id.clone(),
                Role::User,
                self.app_context.base_session_context(),
            ),
            None => ExecutionContext::anonymous(self.app_context.base_session_context()),
        }
    }

    fn mutation_user_id<'a>(
        table_type: TableType,
        tenant_context: &'a TenantContext,
    ) -> Result<&'a UserId, KalamPgError> {
        match table_type {
            TableType::User | TableType::Stream => {
                tenant_context.effective_user_id().ok_or_else(|| {
                    KalamPgError::Validation(format!(
                        "user_id is required for {} mutations",
                        table_type
                    ))
                })
            }
            TableType::Shared => Ok(system_user_id()),
            _ => Err(KalamPgError::Unsupported(format!(
                "{} mutations are not supported in embedded mode",
                table_type
            ))),
        }
    }
}

#[async_trait::async_trait]
impl KalamBackendExecutor for EmbeddedKalamRuntime {
    async fn scan(&self, request: ScanRequest) -> Result<ScanResponse, KalamPgError> {
        request.validate()?;

        let provider =
            self.app_context.schema_registry().get_provider(&request.table_id).ok_or_else(
                || {
                    KalamPgError::Execution(format!(
                        "table provider not registered for {}",
                        request.table_id
                    ))
                },
            )?;

        let schema = provider.schema();
        let projection = request
            .projection
            .as_ref()
            .map(|columns| {
                columns
                    .iter()
                    .map(|column_name| {
                        schema
                            .fields()
                            .iter()
                            .position(|field| field.name() == column_name)
                            .ok_or_else(|| {
                                KalamPgError::Validation(format!(
                                    "projection column '{}' not found on {}",
                                    column_name, request.table_id
                                ))
                            })
                    })
                    .collect::<Result<Vec<usize>, KalamPgError>>()
            })
            .transpose()?;

        let exec_ctx = self.execution_context(&request.tenant_context);
        let session = exec_ctx.create_session_with_user();
        let plan = provider
            .scan(&session.state(), projection.as_ref(), &request.filters, request.limit)
            .await?;
        let batches = collect(plan, session.task_ctx()).await?;

        Ok(ScanResponse::new(batches))
    }

    async fn insert(&self, request: InsertRequest) -> Result<MutationResponse, KalamPgError> {
        request.validate()?;

        let provider = self
            .app_context
            .schema_registry()
            .get_kalam_provider(&request.table_id)
            .ok_or_else(|| {
                KalamPgError::Execution(format!(
                    "kalam provider not registered for {}",
                    request.table_id
                ))
            })?;
        let user_id = Self::mutation_user_id(request.table_type, &request.tenant_context)?;
        let inserted = provider
            .insert_rows(user_id, request.rows)
            .await
            .map_err(|err| KalamPgError::Execution(err.to_string()))?;

        Ok(MutationResponse {
            affected_rows: inserted as u64,
        })
    }

    async fn update(&self, request: UpdateRequest) -> Result<MutationResponse, KalamPgError> {
        request.validate()?;

        let provider = self
            .app_context
            .schema_registry()
            .get_kalam_provider(&request.table_id)
            .ok_or_else(|| {
                KalamPgError::Execution(format!(
                    "kalam provider not registered for {}",
                    request.table_id
                ))
            })?;
        let user_id = Self::mutation_user_id(request.table_type, &request.tenant_context)?;
        let updated = provider
            .update_row_by_pk(user_id, &request.pk_value, request.updates)
            .await
            .map_err(|err| KalamPgError::Execution(err.to_string()))?;

        Ok(MutationResponse {
            affected_rows: updated as u64,
        })
    }

    async fn delete(&self, request: DeleteRequest) -> Result<MutationResponse, KalamPgError> {
        request.validate()?;

        let provider = self
            .app_context
            .schema_registry()
            .get_kalam_provider(&request.table_id)
            .ok_or_else(|| {
                KalamPgError::Execution(format!(
                    "kalam provider not registered for {}",
                    request.table_id
                ))
            })?;
        let user_id = Self::mutation_user_id(request.table_type, &request.tenant_context)?;
        let deleted = provider
            .delete_row_by_pk(user_id, &request.pk_value)
            .await
            .map_err(|err| KalamPgError::Execution(err.to_string()))?;

        Ok(MutationResponse {
            affected_rows: deleted as u64,
        })
    }
}
