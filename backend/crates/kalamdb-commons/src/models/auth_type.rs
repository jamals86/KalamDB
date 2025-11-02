use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Enum representing authentication types in KalamDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub enum AuthType {
    Password,
    OAuth,
    Internal,
}

impl AuthType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthType::Password => "password",
            AuthType::OAuth => "oauth",
            AuthType::Internal => "internal",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "password" => Some(AuthType::Password),
            "oauth" => Some(AuthType::OAuth),
            "internal" => Some(AuthType::Internal),
            _ => None,
        }
    }
}

impl fmt::Display for AuthType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for AuthType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "password" => AuthType::Password,
            "oauth" => AuthType::OAuth,
            "internal" => AuthType::Internal,
            _ => AuthType::Password,
        }
    }
}

impl From<String> for AuthType {
    fn from(s: String) -> Self {
        AuthType::from(s.as_str())
    }
}
