use crate::app_context::AppContext;
use crate::error::KalamDbError;
use kalamdb_commons::models::UserId;
use kalamdb_commons::Role;
use kalamdb_session::can_impersonate_role;
use std::sync::Arc;

/// Core service for SQL "execute as user" resolution and authorization.
pub struct SqlImpersonationService {
    app_context: Arc<AppContext>,
}

impl SqlImpersonationService {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }

    /// Resolve a target username and authorize actor -> target impersonation.
    ///
    /// Returns the canonical target user_id on success.
    pub fn resolve_execute_as_user(
        &self,
        actor_user_id: &UserId,
        actor_role: Role,
        target_username: &str,
    ) -> Result<UserId, KalamDbError> {
        let users_provider = self.app_context.system_tables().users();
        let target_user = if let Some(user) =
            users_provider.get_user_by_username(target_username).map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to resolve EXECUTE AS USER target '{}' by username: {}",
                    target_username, e
                ))
            })? {
            user
        } else {
            users_provider
                .get_user_by_id(&UserId::from(target_username.to_string()))
                .map_err(|e| {
                    KalamDbError::InvalidOperation(format!(
                        "Failed to resolve EXECUTE AS USER target '{}' by user_id: {}",
                        target_username, e
                    ))
                })?
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!(
                        "EXECUTE AS USER target '{}' was not found by username or user_id",
                        target_username
                    ))
                })?
        };

        // No-op impersonation is always allowed.
        if &target_user.user_id == actor_user_id {
            return Ok(target_user.user_id);
        }

        if !can_impersonate_role(actor_role, target_user.role) {
            return Err(KalamDbError::Unauthorized(format!(
                "Role {:?} cannot execute as user '{}' with role {:?}",
                actor_role, target_username, target_user.role
            )));
        }

        Ok(target_user.user_id)
    }
}
