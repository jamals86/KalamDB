//! Path resolution for storage paths (StorageCached only).
//!
//! This module is intentionally scoped to StorageCached usage to avoid
//! duplicate path logic elsewhere in the crate.

use kalamdb_commons::models::{TableId, UserId};
use std::borrow::Cow;

/// Template resolution utilities for storage path templates.
pub(crate) struct TemplateResolver;

impl TemplateResolver {
    /// Resolve static placeholders in a storage path template.
    ///
    /// Called once at table creation time. Substitutes:
    /// - `{namespace}` → namespace ID
    /// - `{tableName}` → table name
    ///
    /// Leaves dynamic placeholders (`{userId}`, `{shard}`) for runtime resolution.
    pub(crate) fn resolve_static_placeholders(raw_template: &str, table_id: &TableId) -> String {
        // Normalize legacy placeholder variants first
        let canonical = Self::normalize_template_placeholders(raw_template);

        // Substitute static placeholders
        canonical
            .replace("{namespace}", table_id.namespace_id().as_str())
            .replace("{tableName}", table_id.table_name().as_str())
    }

    /// Resolve dynamic placeholders in a partially-resolved template.
    pub(crate) fn resolve_dynamic_placeholders<'a>(
        template: &'a str,
        user_id: &UserId,
    ) -> Cow<'a, str> {
        let needs_user_sub = template.contains("{userId}");

        if !needs_user_sub {
            return Cow::Borrowed(template);
        }

        let result = template.replace("{userId}", user_id.as_str());
        Cow::Owned(result)
    }

    /// Normalize legacy placeholder variants to canonical names.
    #[inline]
    pub(crate) fn normalize_template_placeholders(template: &str) -> Cow<'_, str> {
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

/// Unified path resolver for StorageCached operations.
pub(crate) struct PathResolver;

impl PathResolver {
    /// Resolve the path prefix for cleanup operations.
    pub(crate) fn resolve_cleanup_prefix(template: &str) -> Cow<'_, str> {
        if let Some((prefix, _)) = template.split_once("{userId}") {
            return Cow::Owned(prefix.trim_end_matches('/').to_string());
        }

        if let Some((prefix, _)) = template.split_once("{shard}") {
            return Cow::Owned(prefix.trim_end_matches('/').to_string());
        }

        Cow::Borrowed(template.trim_end_matches('/'))
    }
}
