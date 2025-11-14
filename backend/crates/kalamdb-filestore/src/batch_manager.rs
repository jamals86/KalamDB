use crate::error::{FilestoreError, Result};
use crate::path_utils::PathUtils;
use std::path::{Path, PathBuf};

/// Metadata about a batch file
#[derive(Debug, Clone)]
pub struct BatchFile {
    pub path: PathBuf,
    pub timestamp_ms: i64,
    pub batch_index: u64,
    pub size_bytes: u64,
}

/// Manages batch files on disk
pub struct BatchManager {
    base_path: PathBuf,
}

impl BatchManager {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// List all batch files in a directory
    pub fn list_batches(&self, directory: &Path) -> Result<Vec<BatchFile>> {
        let full_path = self.base_path.join(directory);
        
        if !full_path.exists() {
            return Ok(Vec::new());
        }

        let mut batches = Vec::new();

        for entry in std::fs::read_dir(full_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("parquet") {
                continue;
            }

            let filename = path.file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| FilestoreError::InvalidBatchFile(
                    format!("Invalid filename: {:?}", path)
                ))?;

            let (timestamp_ms, batch_index) = PathUtils::parse_batch_filename(filename)?;
            let metadata = entry.metadata()?;

            batches.push(BatchFile {
                path,
                timestamp_ms,
                batch_index,
                size_bytes: metadata.len(),
            });
        }

        // Sort by timestamp, then by batch index
        batches.sort_by_key(|b| (b.timestamp_ms, b.batch_index));

        Ok(batches)
    }

    /// Delete batch files older than a given timestamp
    pub fn delete_batches_before(&self, directory: &Path, cutoff_timestamp_ms: i64) -> Result<usize> {
        let batches = self.list_batches(directory)?;
        let mut deleted_count = 0;

        for batch in batches {
            if batch.timestamp_ms < cutoff_timestamp_ms {
                std::fs::remove_file(&batch.path)?;
                deleted_count += 1;
                log::info!("Deleted old batch file: {:?}", batch.path);
            }
        }

        Ok(deleted_count)
    }

    /// Get total size of all batch files in a directory
    pub fn get_total_size(&self, directory: &Path) -> Result<u64> {
        let batches = self.list_batches(directory)?;
        Ok(batches.iter().map(|b| b.size_bytes).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_manager_creation() {
        let manager = BatchManager::new("/tmp/test");
        assert_eq!(manager.base_path, PathBuf::from("/tmp/test"));
    }
}
