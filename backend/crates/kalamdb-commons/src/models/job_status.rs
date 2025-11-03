use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Enum representing job execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum JobStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "running" => Some(JobStatus::Running),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            "cancelled" => Some(JobStatus::Cancelled),
            _ => None,
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for JobStatus {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Failed,
        }
    }
}

impl From<String> for JobStatus {
    fn from(s: String) -> Self {
        JobStatus::from(s.as_str())
    }
}
