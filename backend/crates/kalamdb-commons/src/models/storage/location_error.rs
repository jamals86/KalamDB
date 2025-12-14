use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageLocationConfigError {
    #[error("storage config_json is missing")]
    MissingConfigJson,

    #[error("failed to parse storage config_json: {0}")]
    InvalidJson(String),
}
