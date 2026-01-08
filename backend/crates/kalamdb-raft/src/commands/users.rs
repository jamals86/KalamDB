//! Users group commands (user accounts and authentication)

use chrono::{DateTime, Utc};
use kalamdb_commons::models::UserId;
use kalamdb_commons::types::User;
use serde::{Deserialize, Serialize};

/// Commands for the users Raft group
///
/// Handles: user accounts, authentication, roles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsersCommand {
    /// Create a new user
    CreateUser {
        user: User,
    },

    /// Update user information
    UpdateUser {
        user: User,
    },

    /// Soft-delete a user
    DeleteUser {
        user_id: UserId,
        deleted_at: DateTime<Utc>,
    },

    /// Update last login timestamp
    RecordLogin {
        user_id: UserId,
        logged_in_at: DateTime<Utc>,
    },

    /// Lock/unlock a user account
    SetLocked {
        user_id: UserId,
        locked_until: Option<i64>,
        updated_at: DateTime<Utc>,
    },
}

/// Response from user commands
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum UsersResponse {
    #[default]
    Ok,
    UserCreated {
        user_id: UserId,
    },
    Error {
        message: String,
    },
}

impl UsersResponse {
    /// Create an error response with the given message
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error { message: msg.into() }
    }

    /// Returns true if this is not an error response
    pub fn is_ok(&self) -> bool {
        !matches!(self, Self::Error { .. })
    }
}