use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use kalamdb_commons::models::UserName;

use crate::helpers::basic_auth;

use super::types::AuthRequest;

/// Extract username from auth request for audit logging.
pub fn extract_username_for_audit(request: &AuthRequest) -> UserName {
    match request {
        AuthRequest::Header(header) => {
            if header.starts_with("Basic ") {
                basic_auth::parse_basic_auth_header(header)
                    .ok()
                    .and_then(|(u, _)| UserName::try_new(u).ok())
                    .unwrap_or_else(|| UserName::from("unknown"))
            } else if header.starts_with("Bearer ") {
                extract_jwt_username_unsafe(header.strip_prefix("Bearer ").unwrap_or(""))
            } else {
                UserName::from("unknown")
            }
        },
        AuthRequest::Credentials { username, .. } => {
            UserName::try_new(username.clone()).unwrap_or_else(|_| UserName::from("unknown"))
        },
        AuthRequest::Jwt { token } => extract_jwt_username_unsafe(token),
    }
}

fn extract_jwt_username_unsafe(token: &str) -> UserName {
    let mut parts = token.splitn(3, '.');
    let _header = parts.next();
    let payload = parts.next();
    let signature = parts.next();
    if payload.is_none() || signature.is_none() {
        return UserName::from("unknown");
    }

    if let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(payload.unwrap()) {
        if let Ok(payload_str) = String::from_utf8(payload_bytes) {
            if let Ok(claims) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                if let Some(username) = claims.get("username").and_then(|v| v.as_str()) {
                    return UserName::try_new(username.to_string())
                        .unwrap_or_else(|_| UserName::from("unknown"));
                }
                if let Some(sub) = claims.get("sub").and_then(|v| v.as_str()) {
                    return UserName::try_new(sub.to_string())
                        .unwrap_or_else(|_| UserName::from("unknown"));
                }
            }
        }
    }
    UserName::from("unknown")
}
