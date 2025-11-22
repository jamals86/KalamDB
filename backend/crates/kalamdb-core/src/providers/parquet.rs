use crate::error::KalamDbError;
use crate::manifest::ManifestAccessPlanner;
use crate::providers::core::TableProviderCore;
use crate::schema_registry::TableType;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::logical_expr::Expr;
use kalamdb_commons::models::UserId;
use kalamdb_commons::types::ManifestFile;
use kalamdb_commons::TableId;
use std::path::PathBuf;

/// Shared helper for loading Parquet batches via ManifestAccessPlanner.
pub(crate) fn scan_parquet_files_as_batch(
    core: &TableProviderCore,
    table_id: &TableId,
    table_type: TableType,
    user_id: Option<&UserId>,
    schema: SchemaRef,
    filter: Option<&Expr>,
) -> Result<RecordBatch, KalamDbError> {
    let namespace = table_id.namespace_id();
    let table = table_id.table_name();
    let scope_label = user_id
        .map(|uid| format!("user={}", uid.as_str()))
        .unwrap_or_else(|| format!("scope={}", table_type.as_str()));

    let storage_path = core
        .app_context
        .schema_registry()
        .get_storage_path(table_id, user_id, None)?;
    let storage_dir = PathBuf::from(&storage_path);

    if !storage_dir.exists() {
        log::trace!(
            "No Parquet directory exists ({}) for table {}.{} - returning empty batch",
            scope_label,
            namespace.as_str(),
            table.as_str()
        );
        return Ok(RecordBatch::new_empty(schema.clone()));
    }

    let manifest_cache_service = core.app_context.manifest_cache_service();
    let cache_result = manifest_cache_service.get_or_load(table_id, user_id);
    let mut manifest_opt: Option<ManifestFile> = None;
    let mut use_degraded_mode = false;

    match &cache_result {
        Ok(Some(entry)) => match ManifestFile::from_json(&entry.manifest_json) {
            Ok(manifest) => {
                if let Err(e) = manifest.validate() {
                    log::warn!(
                        "⚠️  [MANIFEST CORRUPTION] table={}.{} {} error={} | Triggering rebuild",
                        namespace.as_str(),
                        table.as_str(),
                        scope_label,
                        e
                    );
                    use_degraded_mode = true;
                    let manifest_service = core.app_context.manifest_service();
                    let ns = namespace.clone();
                    let tbl = table.clone();
                    let uid = user_id.cloned();
                    let scope_for_spawn = scope_label.clone();
                    let manifest_table_type = table_type.clone();
                    let table_id_for_spawn = table_id.clone();
                    tokio::spawn(async move {
                        log::info!(
                            "🔧 [MANIFEST REBUILD STARTED] table={}.{} {}",
                            ns.as_str(),
                            tbl.as_str(),
                            scope_for_spawn
                        );
                        match manifest_service.rebuild_manifest(
                            &table_id_for_spawn,
                            manifest_table_type,
                            uid.as_ref(),
                        ) {
                            Ok(_) => {
                                log::info!(
                                    "✅ [MANIFEST REBUILD COMPLETED] table={}.{} {}",
                                    ns.as_str(),
                                    tbl.as_str(),
                                    scope_for_spawn
                                );
                            }
                            Err(e) => {
                                log::error!(
                                    "❌ [MANIFEST REBUILD FAILED] table={}.{} {} error={}",
                                    ns.as_str(),
                                    tbl.as_str(),
                                    scope_for_spawn,
                                    e
                                );
                            }
                        }
                    });
                } else {
                    manifest_opt = Some(manifest);
                }
            }
            Err(e) => {
                log::warn!(
                    "⚠️  Failed to parse manifest JSON for table={}.{} {}: {} | Using degraded mode",
                    namespace.as_str(),
                    table.as_str(),
                    scope_label,
                    e
                );
                use_degraded_mode = true;
            }
        },
        Ok(None) => {
            log::debug!(
                "⚠️  Manifest cache MISS | table={}.{} | {} | fallback=directory_scan",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            use_degraded_mode = true;
        }
        Err(e) => {
            log::warn!(
                "⚠️  Manifest cache ERROR | table={}.{} | {} | error={} | fallback=directory_scan",
                namespace.as_str(),
                table.as_str(),
                scope_label,
                e
            );
            use_degraded_mode = true;
        }
    }

    if let Some(ref _manifest) = manifest_opt {
        // log::debug!(
        //     "✅ Manifest cache HIT | table={}.{} | {} | batches={}",
        //     namespace.as_str(),
        //     table.as_str(),
        //     scope_label,
        //     manifest.batches.len()
        // );
    }

    let planner = ManifestAccessPlanner::new();
    let (min_seq, max_seq) = filter
        .map(|f| crate::providers::helpers::extract_seq_bounds_from_filter(f))
        .unwrap_or((None, None));
    let seq_range = match (min_seq, max_seq) {
        (Some(min), Some(max)) => Some((min.as_i64(), max.as_i64())),
        _ => None,
    };

    let (combined, (total_batches, skipped, scanned)) = planner.scan_parquet_files(
        manifest_opt.as_ref(),
        &storage_dir,
        seq_range,
        use_degraded_mode,
        schema.clone(),
    )?;

    if total_batches > 0 {
        log::debug!(
            "[Manifest Pruning] table={}.{} {} batches_total={} skipped={} scanned={} rows={}",
            namespace.as_str(),
            table.as_str(),
            scope_label,
            total_batches,
            skipped,
            scanned,
            combined.num_rows()
        );
    }

    Ok(combined)
}
