use crate::config::OidcConfig;
use crate::error::OidcError;
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// OIDC JWT validator with JWKS caching.
///
/// Each `OidcValidator` is bound to a single issuer configuration and
/// maintains an in-process cache of that issuer's public keys (JWKS).
///
/// When a token's `kid` is not found in the cache the validator
/// automatically refreshes the key set (handles key rotation).
#[derive(Clone)]
pub struct OidcValidator {
    config: OidcConfig,
    jwks_cache: Arc<RwLock<HashMap<String, Jwk>>>,
}

impl OidcValidator {
    /// Create a new validator for the given issuer configuration.
    pub fn new(config: OidcConfig) -> Self {
        Self {
            config,
            jwks_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Return a reference to the underlying config.
    pub fn config(&self) -> &OidcConfig {
        &self.config
    }

    /// Validate a JWT using the issuer's JWKS.
    ///
    /// Validation includes:
    /// - Signature verification against the issuer's public key (RS256 default)
    /// - `iss` must match the configured issuer URL
    /// - `aud` must match the configured client ID (if set)
    /// - `exp` must not be in the past
    pub async fn validate<T>(&self, token: &str) -> Result<T, OidcError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut validation = Validation::new(Algorithm::RS256);

        // Accept all asymmetric algorithms the issuer might use
        validation.algorithms = vec![
            Algorithm::RS256,
            Algorithm::RS384,
            Algorithm::RS512,
            Algorithm::PS256,
            Algorithm::PS384,
            Algorithm::PS512,
            Algorithm::ES256,
            Algorithm::ES384,
        ];

        validation.set_issuer(&[&self.config.issuer_url]);

        if let Some(ref cid) = self.config.client_id {
            validation.set_audience(&[cid]);
        } else {
            validation.validate_aud = false;
        }

        self.validate_custom(token, &validation).await
    }

    /// Validate a JWT with a caller-supplied `Validation` configuration.
    ///
    /// Use this when you need non-standard validation rules (e.g. skip `aud`
    /// checks, accept additional algorithms, add `required_spec_claims`).
    pub async fn validate_custom<T>(
        &self,
        token: &str,
        validation: &Validation,
    ) -> Result<T, OidcError>
    where
        T: for<'de> Deserialize<'de>,
    {
        log::debug!("Validating JWT token");

        let header = decode_header(token)?;

        let kid = header.kid.ok_or(OidcError::MissingKid)?;
        log::debug!("Token kid: {}, alg: {:?}", kid, header.alg);

        let jwk = self.get_jwk(&kid).await?;
        log::debug!("Found matching key with kid: {}", kid);

        let decoding_key = DecodingKey::from_jwk(&jwk)
            .map_err(|e| OidcError::InvalidKeyFormat(e.to_string()))?;

        // Pin validation to the exact algorithm in the token header.
        // This is required for jsonwebtoken 10.x (especially with aws_lc_rs)
        // because overriding `algorithms` after Validation::new can be unreliable.
        let mut pinned = validation.clone();
        pinned.algorithms = vec![header.alg];

        log::debug!("Calling decode with alg={:?}", header.alg);
        let token_data = decode::<T>(token, &decoding_key, &pinned).map_err(|e| {
            log::warn!("JWT decode failed: kind={:?} msg={}", e.kind(), e);
            OidcError::from(e)
        })?;
        log::debug!("Token verified successfully");

        Ok(token_data.claims)
    }

    /// Look up a JWK by `kid`. Refreshes the cache on miss.
    async fn get_jwk(&self, kid: &str) -> Result<Jwk, OidcError> {
        // Fast path: read lock
        {
            let cache = self.jwks_cache.read().await;
            if let Some(jwk) = cache.get(kid) {
                return Ok(jwk.clone());
            }
        }

        // Cache miss — refresh and retry
        self.refresh_jwks_cache().await?;

        let cache = self.jwks_cache.read().await;
        cache
            .get(kid)
            .cloned()
            .ok_or_else(|| OidcError::KeyNotFound(kid.to_string()))
    }

    /// Fetch the JWKS from the issuer and replace the cache if keys changed.
    pub async fn refresh_jwks_cache(&self) -> Result<(), OidcError> {
        log::info!("Refreshing JWKS cache for {}", self.config.issuer_url);

        let new_jwks = self.fetch_jwks().await?;

        let needs_update = {
            let cache = self.jwks_cache.read().await;
            if new_jwks.keys.len() != cache.len() {
                true
            } else {
                new_jwks.keys.iter().any(|jwk| {
                    jwk.common
                        .key_id
                        .as_ref()
                        .map_or(false, |kid| !cache.contains_key(kid))
                })
            }
        };

        if needs_update {
            let mut new_cache = HashMap::new();
            for jwk in new_jwks.keys {
                if let Some(kid) = jwk.common.key_id.clone() {
                    log::debug!("Caching key: {}", kid);
                    new_cache.insert(kid, jwk);
                }
            }

            let mut cache = self.jwks_cache.write().await;
            *cache = new_cache;
            log::info!(
                "JWKS cache refreshed with {} keys for {}",
                cache.len(),
                self.config.issuer_url
            );
        } else {
            log::debug!("JWKS cache unchanged for {}", self.config.issuer_url);
        }

        Ok(())
    }

    /// HTTP fetch of the issuer's JWKS endpoint.
    async fn fetch_jwks(&self) -> Result<JwkSet, OidcError> {
        log::debug!("Fetching JWKS from: {}", self.config.jwks_uri);

        let response = reqwest::get(&self.config.jwks_uri).await.map_err(|e| {
            OidcError::JwksFetchFailed(format!(
                "Failed to fetch JWKS from '{}': {}",
                self.config.jwks_uri, e
            ))
        })?;

        if !response.status().is_success() {
            return Err(OidcError::JwksFetchFailed(format!(
                "JWKS request to '{}' returned status {}",
                self.config.jwks_uri,
                response.status()
            )));
        }

        let jwks: JwkSet = response.json().await.map_err(|e| {
            OidcError::JwksFetchFailed(format!(
                "Failed to parse JWKS JSON from '{}': {}",
                self.config.jwks_uri, e
            ))
        })?;

        log::debug!("Fetched {} keys from JWKS", jwks.keys.len());
        Ok(jwks)
    }
}
