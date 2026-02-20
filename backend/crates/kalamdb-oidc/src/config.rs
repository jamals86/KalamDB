use crate::error::OidcError;

/// OpenID Connect Discovery document (partial).
#[derive(Debug, serde::Deserialize)]
struct OidcDiscovery {
    jwks_uri: String,
}

/// Configuration for a single OIDC issuer.
///
/// Can be constructed manually (if you already know the `jwks_uri`) or via
/// [`OidcConfig::discover`] which calls the issuer's OIDC Discovery endpoint.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// The issuer URL (e.g. `https://keycloak.example.com/realms/myapp`).
    pub issuer_url: String,

    /// Optional OAuth client ID used for audience validation.
    /// If `None`, audience validation is skipped.
    pub client_id: Option<String>,

    /// The JWKS URI where the issuer publishes its public keys.
    pub jwks_uri: String,
}

impl OidcConfig {
    /// Create a config with explicit parameters (no network call).
    pub fn new(issuer_url: String, client_id: Option<String>, jwks_uri: String) -> Self {
        Self {
            issuer_url,
            client_id,
            jwks_uri,
        }
    }

    /// Create a config by performing OIDC Discovery to resolve the `jwks_uri`.
    ///
    /// Fetches `{issuer_url}/.well-known/openid-configuration` and reads `jwks_uri`.
    pub async fn discover(
        issuer_url: String,
        client_id: Option<String>,
    ) -> Result<Self, OidcError> {
        let jwks_uri = Self::discover_jwks_uri(&issuer_url).await?;
        Ok(Self {
            issuer_url,
            client_id,
            jwks_uri,
        })
    }

    /// Perform OIDC Discovery to retrieve the JWKS URI.
    async fn discover_jwks_uri(issuer_url: &str) -> Result<String, OidcError> {
        let base = issuer_url.trim_end_matches('/');
        let discovery_url = format!("{}/.well-known/openid-configuration", base);

        log::debug!("OIDC discovery: fetching {}", discovery_url);

        let response = reqwest::get(&discovery_url).await.map_err(|e| {
            OidcError::DiscoveryFailed(format!(
                "Failed to fetch OIDC discovery from '{}': {}",
                discovery_url, e
            ))
        })?;

        if !response.status().is_success() {
            return Err(OidcError::DiscoveryFailed(format!(
                "OIDC discovery request to '{}' returned status {}",
                discovery_url,
                response.status()
            )));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();

        if !content_type.starts_with("application/json") {
            return Err(OidcError::DiscoveryFailed(format!(
                "Unexpected Content-Type from '{}': '{}', expected 'application/json'",
                discovery_url, content_type
            )));
        }

        let discovery: OidcDiscovery = response.json().await.map_err(|e| {
            OidcError::DiscoveryFailed(format!(
                "Failed to parse OIDC discovery JSON from '{}': {}",
                discovery_url, e
            ))
        })?;

        log::debug!("OIDC discovery: jwks_uri = {}", discovery.jwks_uri);
        Ok(discovery.jwks_uri)
    }
}
