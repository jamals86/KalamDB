//! Audit log entry for administrative actions.

use crate::models::{
    ids::{AuditLogId, UserId},
    UserName,
};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Audit log entry for administrative actions.
///
/// Captures who performed an action, what they targeted, and contextual metadata.
/// For AS USER impersonation: actor_user_id is the person who initiated the action,
/// subject_user_id is the person being impersonated (if applicable).
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct AuditLogEntry {
    pub audit_id: AuditLogId,
    pub timestamp: i64,
    pub actor_user_id: UserId,
    pub actor_username: UserName,
    pub action: String,
    pub target: String,
    pub details: Option<String>,    // JSON blob for additional context
    pub ip_address: Option<String>, // Connection source (if available)
    pub subject_user_id: Option<UserId>, // User being impersonated (AS USER operations)
}
