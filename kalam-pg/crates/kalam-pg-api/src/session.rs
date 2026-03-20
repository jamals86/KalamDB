use kalamdb_commons::models::UserId;
use serde::{Deserialize, Serialize};

/// Session and tenant context extracted by the FDW before execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantContext {
    explicit_user_id: Option<UserId>,
    session_user_id: Option<UserId>,
}

impl TenantContext {
    /// Create an anonymous tenant context.
    pub fn anonymous() -> Self {
        Self::default()
    }

    /// Create a tenant context with the same explicit and session user identity.
    pub fn with_user_id(user_id: UserId) -> Self {
        Self {
            explicit_user_id: Some(user_id.clone()),
            session_user_id: Some(user_id),
        }
    }

    /// Create a tenant context with independent explicit and session user identities.
    pub fn new(explicit_user_id: Option<UserId>, session_user_id: Option<UserId>) -> Self {
        Self {
            explicit_user_id,
            session_user_id,
        }
    }

    /// Returns the explicit `_userid` supplied by the query when present.
    pub fn explicit_user_id(&self) -> Option<&UserId> {
        self.explicit_user_id.as_ref()
    }

    /// Returns the user id supplied by the PostgreSQL session when present.
    pub fn session_user_id(&self) -> Option<&UserId> {
        self.session_user_id.as_ref()
    }

    /// Returns the effective user id after combining explicit and session identity.
    pub fn effective_user_id(&self) -> Option<&UserId> {
        self.explicit_user_id.as_ref().or(self.session_user_id.as_ref())
    }

    /// Validate that the explicit and session identities do not conflict.
    pub fn validate(&self) -> Result<(), kalam_pg_common::KalamPgError> {
        if let (Some(explicit), Some(session)) = (&self.explicit_user_id, &self.session_user_id) {
            if explicit != session {
                return Err(kalam_pg_common::KalamPgError::Validation(format!(
                    "explicit user_id '{}' does not match session user_id '{}'",
                    explicit.as_str(),
                    session.as_str()
                )));
            }
        }
        Ok(())
    }
}
