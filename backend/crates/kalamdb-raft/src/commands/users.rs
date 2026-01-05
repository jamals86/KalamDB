//! Users group commands (user accounts and authentication)

use chrono::{DateTime, Utc};
use kalamdb_commons::models::UserId;
use serde::{Deserialize, Serialize};

/// Commands for the users Raft group
///
/// Handles: user accounts, authentication, roles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsersCommand {
    /// Create a new user
    CreateUser {
        user_id: UserId,
        username: String,
        password_hash: String,
        role: String,
        created_at: DateTime<Utc>,
    },

    /// Update user information
    UpdateUser {
        user_id: UserId,
        username: Option<String>,
        password_hash: Option<String>,
        role: Option<String>,
        updated_at: DateTime<Utc>,
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
        locked: bool,
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
    pub fn error(msg: impl Into<String>) -> Self {
        UsersResponse::Error { message: msg.into() }
    }

    pub fn is_ok(&self) -> bool {
        !matches!(self, UsersResponse::Error { .. })
    }
}
