//! Secure client IP extraction with anti-spoofing protection
//!
//! This module provides secure extraction of client IP addresses from HTTP requests,
//! with protection against header spoofing attacks that attempt to bypass localhost checks.

use actix_web::HttpRequest;
use kalamdb_commons::models::ConnectionInfo;
use log::warn;

/// Extract client IP address with security checks against header spoofing
///
/// # Security Model
///
/// 1. **X-Forwarded-For Validation**: Rejects localhost values in X-Forwarded-For header
///    - Prevents attackers from spoofing `X-Forwarded-For: 127.0.0.1` to bypass security
///    - If localhost is detected in header, falls back to peer_addr (real IP)
///
/// 2. **Trust Hierarchy**:
///    - First: X-Forwarded-For (if present and not localhost)
///    - Fallback: peer_addr (direct connection IP)
///
/// 3. **Attack Prevention**:
///    - Blocks: `127.0.0.1`, `::1`, `127.*`, `localhost` in X-Forwarded-For
///    - Logs warning when spoofing attempt is detected
///
/// # Examples
///
/// ```rust
/// use actix_web::HttpRequest;
/// use kalamdb_auth::extract_client_ip_secure;
///
/// fn handler(req: HttpRequest) {
///     let client_ip = extract_client_ip_secure(&req);
///     println!("Client IP: {:?}", client_ip);
/// }
/// ```
pub fn extract_client_ip_secure(req: &HttpRequest) -> ConnectionInfo {
    let peer_addr = req.peer_addr().map(|addr| addr.ip());

    // Trust X-Forwarded-For only when the direct peer is loopback (trusted local reverse proxy).
    if peer_addr.is_some_and(|ip| ip.is_loopback()) {
        if let Some(forwarded_for) = req.headers().get("X-Forwarded-For") {
            if let Ok(header_value) = forwarded_for.to_str() {
                // Take first IP in comma-separated list (original client)
                let first_ip = header_value.split(',').next().unwrap_or("").trim();

                // Security check: Reject localhost values in X-Forwarded-For
                // This prevents bypass attempts like: X-Forwarded-For: 127.0.0.1
                if is_localhost_address(first_ip) {
                    warn!(
                        "Security: Rejected localhost value in trusted X-Forwarded-For header: '{}'. Using peer_addr instead.",
                        first_ip
                    );
                } else if !first_ip.is_empty() {
                    return ConnectionInfo::new(Some(first_ip.to_string()));
                }
            }
        }
    } else if req.headers().contains_key("X-Forwarded-For") {
        warn!("Security: Ignoring X-Forwarded-For from non-loopback peer {:?}", peer_addr);
    }

    // Fallback to peer address (direct TCP connection)
    peer_addr
        .map(|ip| ConnectionInfo::new(Some(ip.to_string())))
        .unwrap_or_else(|| ConnectionInfo::new(None))
}

/// Check if an IP address string represents localhost
///
/// Detects all common localhost representations:
/// - IPv4: 127.0.0.1, 127.*, 127.x.x.x
/// - IPv6: ::1
/// - Hostname: localhost
///
/// # Examples
///
/// ```rust
/// use kalamdb_auth::is_localhost_address;
///
/// assert!(is_localhost_address("127.0.0.1"));
/// assert!(is_localhost_address("127.5.8.3"));
/// assert!(is_localhost_address("::1"));
/// assert!(is_localhost_address("localhost"));
/// assert!(!is_localhost_address("192.168.1.1"));
/// ```
pub fn is_localhost_address(ip: &str) -> bool {
    ip == "127.0.0.1"
        || ip == "::1"
        || ip.starts_with("127.")
        || ip.eq_ignore_ascii_case("localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_localhost_address() {
        // IPv4 localhost
        assert!(is_localhost_address("127.0.0.1"));
        assert!(is_localhost_address("127.1.2.3"));
        assert!(is_localhost_address("127.255.255.255"));

        // IPv6 localhost
        assert!(is_localhost_address("::1"));

        // Hostname
        assert!(is_localhost_address("localhost"));
        assert!(is_localhost_address("LOCALHOST"));
        assert!(is_localhost_address("LocalHost"));

        // Non-localhost addresses
        assert!(!is_localhost_address("192.168.1.1"));
        assert!(!is_localhost_address("10.0.0.1"));
        assert!(!is_localhost_address("8.8.8.8"));
        assert!(!is_localhost_address("::2"));
        assert!(!is_localhost_address(""));
    }

    #[test]
    fn test_localhost_bypass_attempt() {
        // These should all be detected as localhost (bypass attempts)
        let bypass_attempts = vec![
            "127.0.0.1",
            "127.1.1.1",
            "127.255.255.254",
            "::1",
            "localhost",
            "LOCALHOST",
        ];

        for attempt in bypass_attempts {
            assert!(
                is_localhost_address(attempt),
                "Failed to detect localhost bypass attempt: {}",
                attempt
            );
        }
    }
}
