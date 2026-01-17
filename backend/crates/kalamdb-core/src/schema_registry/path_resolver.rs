use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::cached_table_data::CachedTableData;
use kalamdb_commons::models::{StorageId, TableId, UserId};
use kalamdb_commons::schemas::TableType;
use std::borrow::Cow;
use std::sync::Arc;

/// Helper for resolving storage paths.
///
/// # Performance
/// - Uses `Cow<str>` to avoid allocations when no placeholder substitution is needed
/// - Checks for placeholder existence before performing replacements
/// - Reuses computed relative paths across method calls
pub struct PathResolver;

impl PathResolver {
    /// Resolve the relative storage path (within storage, without base directory).
    ///
    /// This is the path used for manifest cache `source_path` and other storage-relative references.
    /// Format depends on storage template configuration (user_tables_template / shared_tables_template).
    ///
    /// Returns: e.g., "chat/messages/root" or "chat/messages/shared" (without manifest.json suffix)
    #[inline]
    pub fn get_relative_storage_path(
        data: &Arc<CachedTableData>,
        user_id: Option<&UserId>,
        shard: Option<u32>,
    ) -> Result<String, KalamDbError> {
        let template = &data.storage_path_template;

        // Fast path: no substitutions needed
        let needs_user_sub = user_id.is_some() && template.contains("{userId}");
        let needs_shard_sub = shard.is_some() && template.contains("{shard}");

        let relative: Cow<'_, str> = if !needs_user_sub && !needs_shard_sub {
            // No dynamic placeholders to substitute - avoid clone
            Cow::Borrowed(template.as_str())
        } else {
            let mut result = template.clone();

            // Substitute {userId} placeholder
            if let Some(uid) = user_id {
                if needs_user_sub {
                    result = result.replace("{userId}", uid.as_str());
                }
            }

            // Substitute {shard} placeholder
            if let Some(shard_num) = shard {
                if needs_shard_sub {
                    result = result.replace("{shard}", &format!("shard_{}", shard_num));
                }
            }

            Cow::Owned(result)
        };

        // Normalize leading slashes (only if present)
        if relative.starts_with('/') {
            Ok(relative.trim_start_matches('/').to_string())
        } else {
            Ok(relative.into_owned())
        }
    }

    /// Get the relative path to manifest.json within storage.
    ///
    /// Returns: e.g., "chat/messages/root/manifest.json"
    #[inline]
    pub fn get_manifest_relative_path(
        data: &Arc<CachedTableData>,
        user_id: Option<&UserId>,
        shard: Option<u32>,
    ) -> Result<String, KalamDbError> {
        let mut relative = Self::get_relative_storage_path(data, user_id, shard)?;
        relative.push_str("/manifest.json");
        Ok(relative)
    }

    /// Resolve storage path with dynamic placeholders substituted (full path including base directory)
    pub fn get_storage_path(
        app_ctx: &AppContext,
        data: &Arc<CachedTableData>,
        user_id: Option<&UserId>,
        shard: Option<u32>,
    ) -> Result<String, KalamDbError> {
        // Get relative path using the shared method
        let relative = Self::get_relative_storage_path(data, user_id, shard)?;

        // Get base directory (cached in storage registry)
        let base_dir = Self::resolve_base_directory(app_ctx, data.storage_id.as_ref())?;

        // Join base directory with relative path
        Ok(Self::join_paths(&base_dir, &relative))
    }

    /// Resolve base directory from storage configuration.
    ///
    /// This is a hot path - storage registry lookup is cached.
    #[inline]
    fn resolve_base_directory(
        app_ctx: &AppContext,
        storage_id: Option<&StorageId>,
    ) -> Result<String, KalamDbError> {
        let storage_registry = app_ctx.storage_registry();
        let default_base = storage_registry.default_storage_path();

        match storage_id {
            Some(sid) => match storage_registry.get_storage(sid) {
                Ok(Some(storage)) => {
                    let trimmed = storage.base_directory.trim();
                    if trimmed.is_empty() {
                        Ok(default_base.to_string())
                    } else {
                        Ok(trimmed.to_string())
                    }
                },
                _ => Ok(default_base.to_string()),
            },
            None => Ok(default_base.to_string()),
        }
    }

    /// Join base directory with relative path, handling cloud storage URIs.
    #[inline]
    fn join_paths(base_dir: &str, relative: &str) -> String {
        // Check for cloud storage schemes
        if base_dir.starts_with("s3://")
            || base_dir.starts_with("gs://")
            || base_dir.starts_with("gcs://")
            || base_dir.starts_with("az://")
            || base_dir.starts_with("azure://")
        {
            format!("{}/{}", base_dir.trim_end_matches('/'), relative.trim_start_matches('/'))
        } else {
            std::path::PathBuf::from(base_dir).join(relative).to_string_lossy().into_owned()
        }
    }

    /// Resolve partial storage path template for a table.
    ///
    /// This is called during table registration (not on every query), so optimization
    /// is less critical here than in get_relative_storage_path.
    pub fn resolve_storage_path_template(
        app_ctx: &AppContext,
        table_id: &TableId,
        table_type: TableType,
        storage_id: &StorageId,
    ) -> Result<String, KalamDbError> {
        // Fetch storage configuration from registry (cached)
        let storage = app_ctx.storage_registry().get_storage(storage_id)?.ok_or_else(|| {
            KalamDbError::InvalidOperation(format!(
                "Storage '{}' not found while resolving path template",
                storage_id.as_str()
            ))
        })?;

        let raw_template: Cow<'_, str> = match table_type {
            TableType::User => Cow::Borrowed(&storage.user_tables_template),
            TableType::Shared | TableType::Stream => Cow::Borrowed(&storage.shared_tables_template),
            TableType::System => Cow::Borrowed("system/{namespace}/{tableName}"),
        };

        // Normalize legacy placeholder variants to canonical names (only if needed)
        let canonical = Self::normalize_template_placeholders(&raw_template);

        // Substitute static placeholders while leaving dynamic ones ({userId}, {shard}) for later
        let resolved = canonical
            .replace("{namespace}", table_id.namespace_id().as_str())
            .replace("{tableName}", table_id.table_name().as_str());

        Ok(resolved)
    }

    /// Normalize legacy placeholder variants to canonical names.
    ///
    /// Uses early-exit optimization: only performs replacements if legacy placeholders are detected.
    #[inline]
    fn normalize_template_placeholders(template: &str) -> Cow<'_, str> {
        // Fast path: check if any legacy placeholders exist
        // Most templates use canonical names, so this check is usually false
        let has_legacy = template.contains("{table_name}")
            || template.contains("{namespace_id}")
            || template.contains("{namespaceId}")
            || template.contains("{table-id}")
            || template.contains("{namespace-id}")
            || template.contains("{user_id}")
            || template.contains("{user-id}")
            || template.contains("{shard_id}")
            || template.contains("{shard-id}");

        if !has_legacy {
            return Cow::Borrowed(template);
        }

        // Slow path: perform replacements
        Cow::Owned(
            template
                .replace("{table_name}", "{tableName}")
                .replace("{namespace_id}", "{namespace}")
                .replace("{namespaceId}", "{namespace}")
                .replace("{table-id}", "{tableName}")
                .replace("{namespace-id}", "{namespace}")
                .replace("{user_id}", "{userId}")
                .replace("{user-id}", "{userId}")
                .replace("{shard_id}", "{shard}")
                .replace("{shard-id}", "{shard}"),
        )
    }
}
