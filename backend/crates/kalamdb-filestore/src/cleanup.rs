use crate::error::{FilestoreError, Result};
use kalamdb_commons::models::schemas::TableType;
use std::fs;
use std::path::{Path, PathBuf};

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Ok(md) = fs::symlink_metadata(&p) {
                if md.is_dir() {
                    total += dir_size(&p);
                } else if md.is_file() {
                    total += md.len();
                }
            }
        }
    }
    total
}

fn remove_path_recursive(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let bytes = dir_size(path);
    match fs::remove_dir_all(path) {
        Ok(_) => bytes,
        Err(_) => 0, // best-effort; caller logs if needed
    }
}

/// Delete Parquet storage for a table based on a partially-resolved template.
///
/// The `relative_template` should already have static placeholders substituted
/// (e.g., `{namespace}`, `{tableName}`) and may still contain dynamic ones
/// (e.g., `{userId}`, `{shard}`).
///
/// - User tables: Deletes all user subdirectories under the `{userId}` prefix.
/// - Shared tables: Deletes the table directory; if `{shard}` is present, deletes
///   the directory prefix before `{shard}`.
/// - Stream tables: No-op (in-memory only).
///
/// Returns the total number of bytes freed (best-effort, computed before deletion).
pub fn delete_parquet_tree_for_table(
    base_dir: impl AsRef<Path>,
    relative_template: &str,
    table_type: TableType,
) -> Result<u64> {
    let base_dir = base_dir.as_ref();
    let mut rel = relative_template.to_string();
    if rel.starts_with('/') {
        rel = rel.trim_start_matches('/').to_string();
    }
    let template_path = base_dir.join(rel);

    let bytes_freed = match table_type {
        TableType::User => {
            // Split at {userId} and remove each subdirectory under the prefix
            let tpl_str = template_path.to_string_lossy().to_string();
            if let Some((prefix, _)) = tpl_str.split_once("{userId}") {
                let base = PathBuf::from(prefix);
                if !base.exists() {
                    0
                } else {
                    let mut total = 0u64;
                    if let Ok(entries) = fs::read_dir(&base) {
                        for entry in entries.flatten() {
                            total += remove_path_recursive(&entry.path());
                        }
                    }
                    let _ = fs::remove_dir(&base);
                    total
                }
            } else {
                // No {userId}; treat like shared
                remove_path_recursive(&template_path)
            }
        }
        TableType::Shared => {
            let tpl_str = template_path.to_string_lossy().to_string();
            let target = if let Some((prefix, _)) = tpl_str.split_once("{shard}") {
                PathBuf::from(prefix)
            } else {
                template_path.clone()
            };
            remove_path_recursive(&target)
        }
        TableType::Stream => 0,
        TableType::System => {
            return Err(FilestoreError::Other(
                "Attempted to delete Parquet for system table".to_string(),
            ));
        }
    };

    Ok(bytes_freed)
}
