use crate::error::KalamDbError;
use crate::schema_registry::cached_table_data::CachedTableData;
use kalamdb_commons::models::{NamespaceId, StorageId, TableName, UserId};
use kalamdb_commons::schemas::TableType;
use std::sync::Arc;

/// Helper for resolving storage paths
pub struct PathResolver;

impl PathResolver {
    /// Resolve storage path with dynamic placeholders substituted
    pub fn get_storage_path(
        data: &Arc<CachedTableData>,
        user_id: Option<&UserId>,
        shard: Option<u32>,
    ) -> Result<String, KalamDbError> {
        // Start with the stored template and substitute dynamic placeholders
        let mut relative = data.storage_path_template.clone();

        // Substitute {userId} placeholder
        if let Some(uid) = user_id {
            relative = relative.replace("{userId}", uid.as_str());
        }

        // Substitute {shard} placeholder
        if let Some(shard_num) = shard {
            relative = relative.replace("{shard}", &format!("shard_{}", shard_num));
        }

        // Resolve base directory from storage configuration or default storage path
        let app_ctx = crate::app_context::AppContext::get();
        let default_base = app_ctx
            .storage_registry()
            .default_storage_path()
            .to_string();

        let base_dir = if let Some(storage_id) = data.storage_id.clone() {
            let storages = app_ctx.system_tables().storages();
            match storages.get_storage(&storage_id) {
                Ok(Some(storage)) => {
                    let trimmed = storage.base_directory.trim();
                    if trimmed.is_empty() {
                        default_base.clone()
                    } else {
                        trimmed.to_string()
                    }
                }
                _ => default_base.clone(),
            }
        } else {
            default_base.clone()
        };

        // Join base directory with relative path (normalize leading slashes)
        let mut rel = relative;
        if rel.starts_with('/') {
            rel = rel.trim_start_matches('/').to_string();
        }

        let full_path = std::path::PathBuf::from(base_dir).join(rel);
        Ok(full_path.to_string_lossy().to_string())
    }

    /// Resolve partial storage path template for a table
    pub fn resolve_storage_path_template(
        namespace: &NamespaceId,
        table_name: &TableName,
        table_type: TableType,
        storage_id: &StorageId,
    ) -> Result<String, KalamDbError> {
        // Fetch storage configuration from system.storages
        let app_ctx = crate::app_context::AppContext::get();
        let storages_provider = app_ctx.system_tables().storages();
        let storage = storages_provider
            .get_storage(storage_id)
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to load storage configuration '{}': {}",
                    storage_id.as_str(),
                    e
                ))
            })?
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "Storage '{}' not found while resolving path template",
                    storage_id.as_str()
                ))
            })?;

        let raw_template = match table_type {
            TableType::User => storage.user_tables_template,
            TableType::Shared | TableType::Stream => storage.shared_tables_template,
            TableType::System => "system/{namespace}/{tableName}".to_string(),
        };

        // Normalize legacy placeholder variants to canonical names
        let canonical = Self::normalize_template_placeholders(&raw_template);

        // Substitute static placeholders while leaving dynamic ones ({userId}, {shard}) for later
        let resolved = canonical
            .replace("{namespace}", namespace.as_str())
            .replace("{tableName}", table_name.as_str());

        Ok(resolved)
    }

    fn normalize_template_placeholders(template: &str) -> String {
        template
            .replace("{table_name}", "{tableName}")
            .replace("{namespace_id}", "{namespace}")
            .replace("{namespaceId}", "{namespace}")
            .replace("{table-id}", "{tableName}")
            .replace("{namespace-id}", "{namespace}")
            .replace("{user_id}", "{userId}")
            .replace("{user-id}", "{userId}")
            .replace("{shard_id}", "{shard}")
            .replace("{shard-id}", "{shard}")
    }
}
