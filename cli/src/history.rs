//! Command history persistence
//!
//! **Implements T089**: CommandHistory with persistence to ~/.kalam/history
//!
//! Maintains command history across sessions for better user experience.

use std::path::{Path, PathBuf};

use crate::error::{CLIError, Result};

/// Command history manager
pub struct CommandHistory {
    /// History file path
    path: PathBuf,

    /// Maximum history size
    max_size: usize,
}

impl CommandHistory {
    /// Create a new history manager
    pub fn new(max_size: usize) -> Self {
        // Default history path
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let path = PathBuf::from(home).join(".kalam").join("history");

        Self { path, max_size }
    }

    /// Create with custom path
    pub fn with_path<P: AsRef<Path>>(path: P, max_size: usize) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            max_size,
        }
    }

    /// Load history from file
    pub fn load(&self) -> Result<Vec<String>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let contents = std::fs::read_to_string(&self.path)
            .map_err(|e| CLIError::HistoryError(format!("Failed to read history file: {}", e)))?;

        // Use a sentinel delimiter that won't appear in SQL
        // Each entry is stored as: base64_encoded_command\n---ENTRY---\n
        let entries: Vec<String> = contents
            .split("\n---ENTRY---\n")
            .filter(|s| !s.trim().is_empty())
            .filter_map(|encoded| {
                // Decode from base64 to preserve newlines and special characters
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded.trim())
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            })
            .collect();

        // Take last max_size entries
        let start_idx = if entries.len() > self.max_size {
            entries.len() - self.max_size
        } else {
            0
        };

        Ok(entries[start_idx..].to_vec())
    }

    /// Save history to file
    pub fn save(&self, history: &[String]) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Take last max_size entries
        let entries: Vec<&String> = history.iter().rev().take(self.max_size).collect();
        let entries: Vec<&String> = entries.into_iter().rev().collect();

        // Encode each entry in base64 to preserve newlines and special characters
        // Use a sentinel delimiter between entries
        let contents = entries
            .iter()
            .map(|s| {
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, s.as_bytes())
            })
            .collect::<Vec<_>>()
            .join("\n---ENTRY---\n");

        std::fs::write(&self.path, contents)
            .map_err(|e| CLIError::HistoryError(format!("Failed to write history file: {}", e)))?;

        Ok(())
    }

    /// Append a command to history
    pub fn append(&self, command: &str) -> Result<()> {
        let mut history = self.load()?;

        // Don't add empty or duplicate consecutive commands
        if command.trim().is_empty() {
            return Ok(());
        }
        if history.last().map(|s| s.as_str()) == Some(command) {
            return Ok(());
        }

        history.push(command.to_string());
        self.save(&history)?;
        Ok(())
    }

    /// Clear history
    pub fn clear(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    /// Get history file path
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn test_history_persistence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let history = CommandHistory::with_path(&path, 100);

        // Save some history
        let commands = vec!["SELECT 1".to_string(), "SELECT 2".to_string()];
        history.save(&commands).unwrap();

        // Load and verify
        let loaded = history.load().unwrap();
        assert_eq!(loaded, commands);
    }

    #[test]
    fn test_history_max_size() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let history = CommandHistory::with_path(&path, 2);

        // Save 3 commands (should keep last 2)
        let commands = vec![
            "SELECT 1".to_string(),
            "SELECT 2".to_string(),
            "SELECT 3".to_string(),
        ];
        history.save(&commands).unwrap();

        let loaded = history.load().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], "SELECT 2");
        assert_eq!(loaded[1], "SELECT 3");
    }

    #[test]
    fn test_append() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let history = CommandHistory::with_path(&path, 100);

        history.append("SELECT 1").unwrap();
        history.append("SELECT 2").unwrap();

        let loaded = history.load().unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn test_clear() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let history = CommandHistory::with_path(&path, 100);

        history.append("SELECT 1").unwrap();
        assert!(path.exists());

        history.clear().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_multiline_commands() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let history = CommandHistory::with_path(&path, 100);

        // Test multiline SQL command with newlines
        let multiline_cmd = "SELECT id,\n       name,\n       email\nFROM users\nWHERE active = true;";
        
        history.append(multiline_cmd).unwrap();
        history.append("SELECT * FROM jobs;").unwrap();
        
        let loaded = history.load().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], multiline_cmd);
        assert_eq!(loaded[1], "SELECT * FROM jobs;");
        
        // Verify the multiline command preserved all newlines
        assert!(loaded[0].contains("\n"));
        assert_eq!(loaded[0].matches('\n').count(), 4);
    }

    #[test]
    fn test_multiline_with_special_chars() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let history = CommandHistory::with_path(&path, 100);

        // Test command with special characters and newlines
        let special_cmd = "INSERT INTO messages (content)\nVALUES ('Hello\nWorld!'),\n       ('Test\tmessage\nwith\\special chars');";
        
        history.append(special_cmd).unwrap();
        
        let loaded = history.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], special_cmd);
    }
}
