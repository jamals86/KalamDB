//! File-based credential storage for CLI
//!
//! **Implements T119**: FileCredentialStore for persistent credential storage
//!
//! Stores credentials in TOML format at `~/.config/kalamdb/credentials.toml`
//! with secure file permissions (0600 on Unix).
//!
//! # Security
//!
//! - File permissions set to 0600 (owner read/write only) on Unix
//! - Passwords stored in plain text (consider using OS keyring for production)
//! - File location: `~/.config/kalamdb/credentials.toml`
//!
//! # File Format
//!
//! ```toml
//! [instances.local]
//! username = "alice"
//! password = "secret123"
//! server_url = "http://localhost:3000"
//!
//! [instances.production]
//! username = "admin"
//! password = "prod_password"
//! server_url = "https://db.example.com"
//! ```

use kalam_link::credentials::{CredentialStore, Credentials};
use kalam_link::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// File-based credential storage
///
/// Persists credentials to `~/.config/kalamdb/credentials.toml` with
/// secure file permissions.
#[derive(Debug, Clone)]
pub struct FileCredentialStore {
    /// Path to credentials file
    file_path: PathBuf,

    /// In-memory cache of credentials
    cache: HashMap<String, StoredCredential>,
}

/// Stored credential format for TOML serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct StoredCredential {
    username: String,
    password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_url: Option<String>,
}

/// Top-level TOML structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CredentialsFile {
    #[serde(default)]
    instances: HashMap<String, StoredCredential>,
}

impl FileCredentialStore {
    /// Default credentials file path: `~/.config/kalamdb/credentials.toml`
    pub fn default_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("kalamdb").join("credentials.toml")
        } else if let Some(home_dir) = dirs::home_dir() {
            home_dir.join(".config").join("kalamdb").join("credentials.toml")
        } else {
            PathBuf::from(".kalamdb").join("credentials.toml")
        }
    }

    /// Create a new file-based credential store at the default location
    pub fn new() -> kalam_link::Result<Self> {
        Self::with_path(Self::default_path())
    }

    /// Create a new file-based credential store at a custom location
    pub fn with_path(file_path: PathBuf) -> kalam_link::Result<Self> {
        let mut store = Self {
            file_path,
            cache: HashMap::new(),
        };
        store.load_from_disk()?;
        Ok(store)
    }

    /// Load credentials from disk into memory cache
    fn load_from_disk(&mut self) -> kalam_link::Result<()> {
        if !self.file_path.exists() {
            // No file yet, start with empty cache
            self.cache.clear();
            return Ok(());
        }

        let contents = fs::read_to_string(&self.file_path).map_err(|e| {
            kalam_link::KalamLinkError::ConfigurationError(format!(
                "Failed to read credentials file: {}",
                e
            ))
        })?;

        let file: CredentialsFile = toml::from_str(&contents).map_err(|e| {
            kalam_link::KalamLinkError::ConfigurationError(format!(
                "Failed to parse credentials file: {}",
                e
            ))
        })?;

        self.cache = file.instances;
        Ok(())
    }

    /// Save credentials from memory cache to disk
    fn save_to_disk(&self) -> kalam_link::Result<()> {
        let file = CredentialsFile {
            instances: self.cache.clone(),
        };

        let contents = toml::to_string_pretty(&file).map_err(|e| {
            kalam_link::KalamLinkError::ConfigurationError(format!(
                "Failed to serialize credentials: {}",
                e
            ))
        })?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                kalam_link::KalamLinkError::ConfigurationError(format!(
                    "Failed to create credentials directory: {}",
                    e
                ))
            })?;
        }

        // Write file with secure permissions
        fs::write(&self.file_path, contents).map_err(|e| {
            kalam_link::KalamLinkError::ConfigurationError(format!("Failed to write credentials file: {}", e))
        })?;

        // Set file permissions to 0600 (owner read/write only) on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&self.file_path, permissions).map_err(|e| {
                kalam_link::KalamLinkError::ConfigurationError(format!(
                    "Failed to set file permissions: {}",
                    e
                ))
            })?;
        }

        Ok(())
    }

    /// Get the file path used by this store
    pub fn path(&self) -> &Path {
        &self.file_path
    }
}

impl Default for FileCredentialStore {
    fn default() -> Self {
        Self::new().expect("Failed to create default FileCredentialStore")
    }
}

impl CredentialStore for FileCredentialStore {
    fn get_credentials(&self, instance: &str) -> Result<Option<Credentials>> {
        if let Some(stored) = self.cache.get(instance) {
            Ok(Some(Credentials {
                instance: instance.to_string(),
                username: stored.username.clone(),
                password: stored.password.clone(),
                server_url: stored.server_url.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    fn set_credentials(&mut self, credentials: &Credentials) -> Result<()> {
        let stored = StoredCredential {
            username: credentials.username.clone(),
            password: credentials.password.clone(),
            server_url: credentials.server_url.clone(),
        };

        self.cache
            .insert(credentials.instance.clone(), stored);
        self.save_to_disk()?;
        Ok(())
    }

    fn delete_credentials(&mut self, instance: &str) -> Result<()> {
        self.cache.remove(instance);
        self.save_to_disk()?;
        Ok(())
    }

    fn list_instances(&self) -> Result<Vec<String>> {
        Ok(self.cache.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_store() -> (FileCredentialStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("credentials.toml");
        let store = FileCredentialStore::with_path(file_path).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_file_store_basic_operations() {
        let (mut store, _temp_dir) = create_temp_store();

        // Initially empty
        assert_eq!(store.get_credentials("local").unwrap(), None);
        assert!(!store.has_credentials("local").unwrap());

        // Store credentials
        let creds = Credentials::new(
            "local".to_string(),
            "alice".to_string(),
            "secret".to_string(),
        );
        store.set_credentials(&creds).unwrap();

        // Retrieve credentials
        let retrieved = store.get_credentials("local").unwrap();
        assert_eq!(retrieved.as_ref().unwrap().username, "alice");
        assert_eq!(retrieved.as_ref().unwrap().password, "secret");
        assert!(store.has_credentials("local").unwrap());

        // Delete credentials
        store.delete_credentials("local").unwrap();
        assert_eq!(store.get_credentials("local").unwrap(), None);
    }

    #[test]
    fn test_file_store_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("credentials.toml");

        // Create store and add credentials
        {
            let mut store = FileCredentialStore::with_path(file_path.clone()).unwrap();
            let creds = Credentials::new(
                "prod".to_string(),
                "bob".to_string(),
                "password123".to_string(),
            );
            store.set_credentials(&creds).unwrap();
        }

        // Verify file was created
        assert!(file_path.exists());

        // Load store again and verify credentials persisted
        {
            let store = FileCredentialStore::with_path(file_path).unwrap();
            let retrieved = store.get_credentials("prod").unwrap().unwrap();
            assert_eq!(retrieved.username, "bob");
            assert_eq!(retrieved.password, "password123");
        }
    }

    #[test]
    fn test_file_store_multiple_instances() {
        let (mut store, _temp_dir) = create_temp_store();

        let creds1 = Credentials::new(
            "local".to_string(),
            "alice".to_string(),
            "pass1".to_string(),
        );
        let creds2 = Credentials::with_server_url(
            "prod".to_string(),
            "bob".to_string(),
            "pass2".to_string(),
            "https://db.example.com".to_string(),
        );

        store.set_credentials(&creds1).unwrap();
        store.set_credentials(&creds2).unwrap();

        // List instances
        let instances = store.list_instances().unwrap();
        assert_eq!(instances.len(), 2);
        assert!(instances.contains(&"local".to_string()));
        assert!(instances.contains(&"prod".to_string()));

        // Retrieve specific instances
        let local = store.get_credentials("local").unwrap().unwrap();
        assert_eq!(local.username, "alice");
        assert_eq!(local.server_url, None);

        let prod = store.get_credentials("prod").unwrap().unwrap();
        assert_eq!(prod.username, "bob");
        assert_eq!(prod.server_url, Some("https://db.example.com".to_string()));
    }

    #[test]
    fn test_file_store_overwrite() {
        let (mut store, _temp_dir) = create_temp_store();

        let creds1 = Credentials::new(
            "local".to_string(),
            "alice".to_string(),
            "old_pass".to_string(),
        );
        let creds2 = Credentials::new(
            "local".to_string(),
            "alice".to_string(),
            "new_pass".to_string(),
        );

        store.set_credentials(&creds1).unwrap();
        store.set_credentials(&creds2).unwrap();

        let retrieved = store.get_credentials("local").unwrap().unwrap();
        assert_eq!(retrieved.password, "new_pass");
    }

    #[test]
    #[cfg(unix)]
    fn test_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let (mut store, _temp_dir) = create_temp_store();

        let creds = Credentials::new(
            "local".to_string(),
            "alice".to_string(),
            "secret".to_string(),
        );
        store.set_credentials(&creds).unwrap();

        // Check file permissions are 0600
        let metadata = fs::metadata(store.path()).unwrap();
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, 0o600);
    }

    #[test]
    fn test_toml_format() {
        let (mut store, _temp_dir) = create_temp_store();

        let creds1 = Credentials::with_server_url(
            "local".to_string(),
            "alice".to_string(),
            "pass1".to_string(),
            "http://localhost:3000".to_string(),
        );
        let creds2 = Credentials::new(
            "prod".to_string(),
            "bob".to_string(),
            "pass2".to_string(),
        );

        store.set_credentials(&creds1).unwrap();
        store.set_credentials(&creds2).unwrap();

        // Read raw file and verify TOML structure
        let contents = fs::read_to_string(store.path()).unwrap();
        assert!(contents.contains("[instances.local]"));
        assert!(contents.contains("[instances.prod]"));
        assert!(contents.contains("username = \"alice\""));
        assert!(contents.contains("username = \"bob\""));
        assert!(contents.contains("password = \"pass1\""));
        assert!(contents.contains("password = \"pass2\""));
        assert!(contents.contains("server_url = \"http://localhost:3000\""));
    }
}
