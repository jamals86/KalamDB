use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{Result, StreamLogError};

pub(crate) fn parse_log_window(path: &Path) -> Option<u64> {
    let file_name = path.file_name()?.to_string_lossy();
    let trimmed = file_name.strip_suffix(".log")?;
    trimmed.parse::<u64>().ok()
}

pub(crate) fn parse_tmp_log_window(path: &Path) -> Option<u64> {
    let file_name = path.file_name()?.to_string_lossy();
    let trimmed = file_name.strip_suffix(".log.tmp")?;
    trimmed.parse::<u64>().ok()
}

pub(crate) fn visit_dirs<F>(path: &Path, mut visitor: F) -> Result<bool>
where
    F: FnMut(PathBuf) -> Result<bool>,
{
    for entry in fs::read_dir(path).map_err(|e| StreamLogError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| StreamLogError::Io(e.to_string()))?;
        let path = entry.path();
        if path.is_dir() {
            if !visitor(path)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn visit_files<F>(path: &Path, mut visitor: F) -> Result<bool>
where
    F: FnMut(PathBuf) -> Result<bool>,
{
    for entry in fs::read_dir(path).map_err(|e| StreamLogError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| StreamLogError::Io(e.to_string()))?;
        let path = entry.path();
        if path.is_file() {
            if !visitor(path)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn cleanup_empty_dir(path: &Path) {
    if let Ok(mut entries) = fs::read_dir(path) {
        if entries.next().is_none() {
            let _ = fs::remove_dir(path);
        }
    }
}
