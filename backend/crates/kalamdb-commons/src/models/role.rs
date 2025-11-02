use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Enum representing user roles in KalamDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum Role {
    User,
    Service,
    Dba,
    System,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Service => "service",
            Role::Dba => "dba",
            Role::System => "system",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "user" => Some(Role::User),
            "service" => Some(Role::Service),
            "dba" => Some(Role::Dba),
            "system" => Some(Role::System),
            _ => None,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "user" => Role::User,
            "service" => Role::Service,
            "dba" => Role::Dba,
            "system" => Role::System,
            _ => Role::User,
        }
    }
}

impl From<String> for Role {
    fn from(s: String) -> Self {
        Role::from(s.as_str())
    }
}
