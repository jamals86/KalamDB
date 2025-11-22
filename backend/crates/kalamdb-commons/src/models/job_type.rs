use std::fmt;
use std::str::FromStr;

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
    JobCleanup,
    Backup,
    Restore,
    Retention,
    StreamEviction,
    UserCleanup,
    Unknown,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobType::Flush => "flush",
            JobType::Compact => "compact",
            JobType::Cleanup => "cleanup",
            JobType::JobCleanup => "job_cleanup",
            JobType::Backup => "backup",
            JobType::Restore => "restore",
            JobType::Retention => "retention",
            JobType::StreamEviction => "stream_eviction",
            JobType::UserCleanup => "user_cleanup",
            JobType::Unknown => "unknown",
        }
    }

    /// Returns the 2-letter uppercase prefix for JobId generation
    ///
    /// Prefix mapping:
    /// - FL: Flush
    /// - CO: Compact
    /// - CL: Cleanup
    /// - BK: Backup
    /// - RS: Restore
    /// - RT: Retention
    /// - SE: StreamEviction
    /// - UC: UserCleanup
    /// - UN: Unknown
    pub fn short_prefix(&self) -> &'static str {
        match self {
            JobType::Flush => "FL",
            JobType::Compact => "CO",
            JobType::Cleanup => "CL",
            JobType::JobCleanup => "JC",
            JobType::Backup => "BK",
            JobType::Restore => "RS",
            JobType::Retention => "RT",
            JobType::StreamEviction => "SE",
            JobType::UserCleanup => "UC",
            JobType::Unknown => "UN",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "flush" => Some(JobType::Flush),
            "compact" => Some(JobType::Compact),
            "cleanup" => Some(JobType::Cleanup),
            "job_cleanup" => Some(JobType::JobCleanup),
            "backup" => Some(JobType::Backup),
            "restore" => Some(JobType::Restore),
            "retention" => Some(JobType::Retention),
            "stream_eviction" => Some(JobType::StreamEviction),
            "user_cleanup" => Some(JobType::UserCleanup),
            "unknown" => Some(JobType::Unknown),
            _ => None,
        }
    }
}

impl FromStr for JobType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        JobType::from_str_opt(s).ok_or_else(|| format!("Invalid JobType: {}", s))
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
            "job_cleanup" => JobType::JobCleanup,
            "backup" => JobType::Backup,
            "restore" => JobType::Restore,
            "retention" => JobType::Retention,
            "stream_eviction" => JobType::StreamEviction,
            "user_cleanup" => JobType::UserCleanup,
            "unknown" => JobType::Unknown,
            _ => JobType::Unknown,
        }
    }
}

impl From<String> for JobType {
    fn from(s: String) -> Self {
        JobType::from(s.as_str())
    }
}
