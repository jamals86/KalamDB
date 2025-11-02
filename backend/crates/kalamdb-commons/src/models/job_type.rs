use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Enum representing job types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum JobType {
    Flush,
    Compact,
    Cleanup,
    Backup,
    Restore,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobType::Flush => "flush",
            JobType::Compact => "compact",
            JobType::Cleanup => "cleanup",
            JobType::Backup => "backup",
            JobType::Restore => "restore",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "flush" => Some(JobType::Flush),
            "compact" => Some(JobType::Compact),
            "cleanup" => Some(JobType::Cleanup),
            "backup" => Some(JobType::Backup),
            "restore" => Some(JobType::Restore),
            _ => None,
        }
    }
}

impl fmt::Display for JobType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for JobType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "flush" => JobType::Flush,
            "compact" => JobType::Compact,
            "cleanup" => JobType::Cleanup,
            "backup" => JobType::Backup,
            "restore" => JobType::Restore,
            _ => JobType::Flush,
        }
    }
}

impl From<String> for JobType {
    fn from(s: String) -> Self {
        JobType::from(s.as_str())
    }
}
