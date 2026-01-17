// Connection information detection for authentication

/// Connection information for an authenticated request.
///
/// Used to determine whether remote access is allowed based on IP address.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// IP address of the connecting client (as string from actix-web)
    pub remote_addr: Option<String>,
}

impl ConnectionInfo {
    /// Create a new ConnectionInfo from a remote address string.
    ///
    /// # Arguments
    /// * `remote_addr` - The client's remote address (IP:port or just IP)
    ///
    /// # Returns
    /// A new ConnectionInfo instance
    pub fn new(remote_addr: Option<String>) -> Self {
        Self { remote_addr }
    }

    /// Check if the connection is from localhost.
    ///
    /// Handles both IPv4 (127.0.0.1) and IPv6 (::1) loopback addresses.
    ///
    /// # Returns
    /// True if the connection is from localhost, false otherwise
    pub fn is_localhost(&self) -> bool {
        match &self.remote_addr {
            Some(addr) => {
                addr == "127.0.0.1"
                    || addr.starts_with("127.0.0.1:")
                    || addr == "localhost"
                    || addr.starts_with("localhost:")
                    || addr == "::1"
                    || addr.starts_with("[::1]")
            },
            None => false,
        }
    }

    /// Check if remote access should be allowed for this connection.
    ///
    /// # Arguments
    /// * `allow_remote_access` - Whether remote access is enabled globally
    ///
    /// # Returns
    /// Check if remote access should be allowed for this connection.
    ///
    /// Access is always allowed for localhost connections.
    /// For remote connections, access is allowed only if `allow_remote_access` is true.
    ///
    /// # Arguments
    /// * `allow_remote_access` - Whether remote connections are permitted
    ///
    /// # Returns
    /// True if access should be allowed, false otherwise
    pub fn is_access_allowed(&self, allow_remote_access: bool) -> bool {
        self.is_localhost() || allow_remote_access
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_localhost_ipv4() {
        let conn = ConnectionInfo::new(Some("127.0.0.1".to_string()));
        assert!(conn.is_localhost());
    }

    #[test]
    fn test_localhost_ipv4_with_port() {
        let conn = ConnectionInfo::new(Some("127.0.0.1:8080".to_string()));
        assert!(conn.is_localhost());
    }

    #[test]
    fn test_localhost_name() {
        let conn = ConnectionInfo::new(Some("localhost".to_string()));
        assert!(conn.is_localhost());
    }

    #[test]
    fn test_localhost_ipv6() {
        let conn = ConnectionInfo::new(Some("::1".to_string()));
        assert!(conn.is_localhost());
    }

    #[test]
    fn test_localhost_ipv6_with_brackets() {
        let conn = ConnectionInfo::new(Some("[::1]:8080".to_string()));
        assert!(conn.is_localhost());
    }

    #[test]
    fn test_remote_ipv4() {
        let conn = ConnectionInfo::new(Some("192.168.1.100".to_string()));
        assert!(!conn.is_localhost());
    }

    #[test]
    fn test_none_address() {
        let conn = ConnectionInfo::new(None);
        assert!(!conn.is_localhost());
    }

    #[test]
    fn test_access_allowed_localhost() {
        let conn = ConnectionInfo::new(Some("127.0.0.1".to_string()));
        assert!(conn.is_access_allowed(false)); // Localhost always allowed
        assert!(conn.is_access_allowed(true));
    }

    #[test]
    fn test_access_denied_remote() {
        let conn = ConnectionInfo::new(Some("192.168.1.100".to_string()));
        assert!(!conn.is_access_allowed(false)); // Remote access disabled
        assert!(conn.is_access_allowed(true)); // Remote access enabled
    }
}
