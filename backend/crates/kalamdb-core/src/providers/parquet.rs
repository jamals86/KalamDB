use crate::error::KalamDbError;
use crate::manifest::ManifestAccessPlanner;
use crate::providers::core::TableProviderCore;
use crate::schema_registry::{PathResolver, TableType};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::logical_expr::Expr;
use kalamdb_commons::models::UserId;
use kalamdb_commons::types::Manifest;
use kalamdb_commons::TableId;

/// Shared helper for loading Parquet batches via ManifestAccessPlanner.
pub(crate) fn scan_parquet_files_as_batch(
    core: &TableProviderCore,
    table_id: &TableId,
    table_type: TableType,
    user_id: Option<&UserId>,
    schema: SchemaRef,
    filter: Option<&Expr>,
) -> Result<RecordBatch, KalamDbError> {
    let scope_label = user_id
        .map(|uid| format!("user={}", uid.as_str()))
        .unwrap_or_else(|| format!("scope={}", table_type.as_str()));

    // 1. Get CachedTableData
    let cached = core
        .app_context
        .schema_registry()
        .get(table_id)
        .ok_or_else(|| KalamDbError::TableNotFound(format!("Table not found: {}", table_id)))?;

    // 2. Get Storage from registry (cached lookup)
    let storage_id = cached
        .storage_id
        .clone()
        .unwrap_or_else(kalamdb_commons::models::StorageId::local);

    let storage =
        core.app_context.storage_registry().get_storage(&storage_id)?.ok_or_else(|| {
            KalamDbError::InvalidOperation(format!("Storage '{}' not found", storage_id.as_str()))
        })?;

    // 3. Get ObjectStore (cached)
    let object_store = cached.object_store(core.app_context.as_ref())?;

    // 4. Resolve storage path
    let storage_path =
        PathResolver::get_storage_path(core.app_context.as_ref(), &cached, user_id, None)?;

    let manifest_service = core.app_context.manifest_service();
    let cache_result = manifest_service.get_or_load(table_id, user_id);
    let mut manifest_opt: Option<Manifest> = None;
    let mut use_degraded_mode = false;

    match &cache_result {
        Ok(Some(entry)) => {
            let manifest = entry.manifest.clone();
            // Validate manifest using service
            if let Err(e) = manifest_service.validate_manifest(&manifest) {
                log::warn!(
                    "⚠️  [MANIFEST CORRUPTION] table={} {} error={} | Triggering rebuild",
                    table_id,
                    scope_label,
                    e
                );
                // Mark cache entry as stale so sync_state reflects corruption
                if let Err(mark_err) = manifest_service.mark_as_stale(table_id, user_id) {
                    log::warn!(
                        "⚠️  Failed to mark manifest as stale: table={} {} error={}",
                        table_id,
                        scope_label,
                        mark_err
                    );
                }
                use_degraded_mode = true;
                let uid = user_id.cloned();
                let scope_for_spawn = scope_label.clone();
                let manifest_table_type = table_type;
                let table_id_for_spawn = table_id.clone();
                let manifest_service_clone = core.app_context.manifest_service();
                tokio::spawn(async move {
                    log::info!(
                        "🔧 [MANIFEST REBUILD STARTED] table={} {}",
                        table_id_for_spawn,
                        scope_for_spawn
                    );
                    match manifest_service_clone.rebuild_manifest(
                        &table_id_for_spawn,
                        manifest_table_type,
                        uid.as_ref(),
                    ) {
                        Ok(_) => {
                            log::info!(
                                "✅ [MANIFEST REBUILD COMPLETED] table={} {}",
                                table_id_for_spawn,
                                scope_for_spawn
                            );
                        },
                        Err(e) => {
                            log::error!(
                                "❌ [MANIFEST REBUILD FAILED] table={} {} error={}",
                                table_id_for_spawn,
                                scope_for_spawn,
                                e
                            );
                        },
                    }
                });
            } else {
                manifest_opt = Some(manifest);
            }
        },
        Ok(None) => {
            log::debug!(
                "⚠️  Manifest cache MISS | table={} | {} | fallback=directory_scan",
                table_id,
                scope_label
            );
            use_degraded_mode = true;
        },
        Err(e) => {
            log::warn!(
                "⚠️  Manifest cache ERROR | table={} | {} | error={} | fallback=directory_scan",
                table_id,
                scope_label,
                e
            );
            use_degraded_mode = true;
        },
    }

    if let Some(ref _manifest) = manifest_opt {
        // Manifest was found in cache
    }

    let planner = ManifestAccessPlanner::new();
    let (min_seq, max_seq) = filter
        .map(crate::providers::helpers::extract_seq_bounds_from_filter)
        .unwrap_or((None, None));
    let seq_range = match (min_seq, max_seq) {
        (Some(min), Some(max)) => Some((min.as_i64(), max.as_i64())),
        _ => None,
    };

    let (combined, (total_batches, skipped, scanned)) = planner.scan_parquet_files(
        manifest_opt.as_ref(),
        object_store,
        &storage,
        &storage_path,
        seq_range,
        use_degraded_mode,
        schema.clone(),
        table_id,
        &core.app_context,
    )?;

    if total_batches > 0 {
        log::debug!(
            "[Manifest Pruning] table={} {} batches_total={} skipped={} scanned={} rows={}",
            table_id,
            scope_label,
            total_batches,
            skipped,
            scanned,
            combined.num_rows()
        );
    }

    Ok(combined)
}
